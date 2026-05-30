use std::{collections::HashMap, sync::Arc};

use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    domain::{
        rule_engine::{ExportedRuleDesign, RuleEngine, RuntimeRule},
        user::UserId,
    },
    error::AppError,
    infrastructure::user::UserRepository,
    interface::auth::TokenClaims,
    state::RuleStore,
};

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SaveRuleDraftRequest {
    pub name: String,
    #[serde(rename = "playerCount")]
    pub player_count: u8,
    #[serde(default)]
    pub description: String,
    pub design: ExportedRuleDesign,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleDraft {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    #[serde(rename = "playerCount")]
    pub player_count: u8,
    pub description: String,
    pub status: RuleStatus,
    pub design: ExportedRuleDesign,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_rule_id: Option<String>,
    #[serde(rename = "forkedFromRuleId", skip_serializing_if = "Option::is_none")]
    pub forked_from_rule_id: Option<String>,
    #[serde(rename = "rejectReason", skip_serializing_if = "Option::is_none")]
    pub reject_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleStatus {
    /// 作者本地草稿，不在审核队列，市场不可见。
    Draft,
    /// 已提交，等待管理员审核。
    PendingReview,
    /// 审核通过，已发布到市场。
    Published,
    /// 审核被驳回，作者需要修改后重新提交。
    Rejected,
}

impl RuleStatus {
    /// 序列化到 DB 时使用 snake_case 字符串；与 camelCase 序列化值（pendingReview）不同，
    /// 因为现存数据已是 snake_case，避免一次性迁移。
    pub fn to_db_str(self) -> &'static str {
        match self {
            RuleStatus::Draft => "draft",
            RuleStatus::PendingReview => "pending_review",
            RuleStatus::Published => "published",
            RuleStatus::Rejected => "rejected",
        }
    }

    /// 兜底到 Draft：未知值（包括旧数据残留）按草稿处理，避免阻塞读取。
    pub fn from_db_str(value: &str) -> Self {
        match value {
            "pending_review" => RuleStatus::PendingReview,
            "published" => RuleStatus::Published,
            "rejected" => RuleStatus::Rejected,
            _ => RuleStatus::Draft,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PublishedRule {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    #[serde(rename = "playerCount")]
    pub player_count: u8,
    pub description: String,
    pub version: u32,
    pub design: ExportedRuleDesign,
    #[serde(skip_serializing)]
    pub runtime: RuntimeRule,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
}

#[derive(Debug, Default)]
pub struct RuleRepository {
    pub drafts: HashMap<String, RuleDraft>,
    pub published: HashMap<String, PublishedRule>,
}

#[derive(Clone)]
pub struct RulePersistence {
    pub pool: PgPool,
}

#[derive(Debug, Serialize)]
pub struct SaveDraftResponse {
    pub id: String,
    pub status: RuleStatus,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
}

#[derive(Debug, Serialize)]
pub struct PublishRuleResponse {
    #[serde(rename = "ruleId")]
    pub rule_id: String,
    pub version: u32,
    pub status: RuleStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleOption {
    pub id: String,
    pub name: String,
    #[serde(rename = "playerCount")]
    pub player_count: u8,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleDraftSummary {
    pub id: String,
    pub name: String,
    #[serde(rename = "playerCount")]
    pub player_count: u8,
    pub description: String,
    pub status: RuleStatus,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_rule_id: Option<String>,
}

pub async fn list_drafts(
    TokenClaims { user_id, .. }: TokenClaims,
    State(store): State<RuleStore>,
) -> Result<Json<ApiResponse<Vec<RuleDraftSummary>>>, AppError> {
    let guard = store.read().await;
    let mut drafts = guard
        .drafts
        .values()
        .filter(|draft| draft.owner_id == user_id.to_string())
        .map(|draft| RuleDraftSummary {
            id: draft.id.clone(),
            name: draft.name.clone(),
            player_count: draft.player_count,
            description: draft.description.clone(),
            status: draft.status,
            updated_at: draft.updated_at,
            published_rule_id: draft.published_rule_id.clone(),
        })
        .collect::<Vec<_>>();

    drafts.sort_by_key(|draft| std::cmp::Reverse(draft.updated_at));
    Ok(Json(ApiResponse::success(drafts)))
}

pub async fn save_draft(
    TokenClaims { user_id, .. }: TokenClaims,
    State(store): State<RuleStore>,
    State(persistence): State<RulePersistence>,
    Json(payload): Json<SaveRuleDraftRequest>,
) -> Result<Json<ApiResponse<SaveDraftResponse>>, AppError> {
    // 保存草稿时就解析一次，尽早把前端 JSON 中的结构错误反馈给用户。
    RuleEngine::parse(
        payload.name.clone(),
        payload.player_count,
        payload.description.clone(),
        payload.design.clone(),
    )?;

    let now = now_millis();
    let draft = RuleDraft {
        id: uuid::Uuid::new_v4().to_string(),
        owner_id: user_id.to_string(),
        name: payload.name,
        player_count: payload.player_count,
        description: payload.description,
        status: RuleStatus::Draft,
        design: payload.design,
        created_at: now,
        updated_at: now,
        published_rule_id: None,
        forked_from_rule_id: None,
        reject_reason: None,
    };
    let response = SaveDraftResponse {
        id: draft.id.clone(),
        status: draft.status,
        updated_at: draft.updated_at,
    };

    persistence.save_draft(&draft).await?;
    store.write().await.drafts.insert(draft.id.clone(), draft);
    Ok(Json(ApiResponse::success(response)))
}

pub async fn update_draft(
    TokenClaims { user_id, .. }: TokenClaims,
    State(store): State<RuleStore>,
    State(persistence): State<RulePersistence>,
    Path(draft_id): Path<String>,
    Json(payload): Json<SaveRuleDraftRequest>,
) -> Result<Json<ApiResponse<SaveDraftResponse>>, AppError> {
    let runtime = RuleEngine::parse(
        payload.name.clone(),
        payload.player_count,
        payload.description.clone(),
        payload.design.clone(),
    )?;
    drop(runtime);

    let now = now_millis();
    let mut guard = store.write().await;
    let draft = guard.drafts.get_mut(&draft_id).ok_or(AppError::NotFound)?;
    ensure_owner(&draft.owner_id, &user_id.to_string())?;

    draft.name = payload.name;
    draft.player_count = payload.player_count;
    draft.description = payload.description;
    draft.design = payload.design;
    // 编辑动作把任何非草稿状态（pending_review / rejected / published）都拉回 Draft，
    // 强制走"重新提审"的路径，避免市场上线版本被原地改掉或绕过审核。
    if draft.status != RuleStatus::Draft {
        draft.status = RuleStatus::Draft;
        draft.reject_reason = None;
    }
    draft.updated_at = now;
    persistence.save_draft(draft).await?;

    Ok(Json(ApiResponse::success(SaveDraftResponse {
        id: draft.id.clone(),
        status: draft.status,
        updated_at: draft.updated_at,
    })))
}

pub async fn get_draft(
    TokenClaims { user_id, .. }: TokenClaims,
    State(store): State<RuleStore>,
    Path(draft_id): Path<String>,
) -> Result<Json<ApiResponse<RuleDraft>>, AppError> {
    let guard = store.read().await;
    let draft = guard.drafts.get(&draft_id).ok_or(AppError::NotFound)?;
    ensure_owner(&draft.owner_id, &user_id.to_string())?;

    Ok(Json(ApiResponse::success(draft.clone())))
}

pub async fn delete_draft(
    TokenClaims { user_id, .. }: TokenClaims,
    State(store): State<RuleStore>,
    State(persistence): State<RulePersistence>,
    Path(draft_id): Path<String>,
) -> Result<Json<ApiResponse<RuleDraftSummary>>, AppError> {
    let mut guard = store.write().await;
    let draft = guard.drafts.get(&draft_id).ok_or(AppError::NotFound)?;
    ensure_owner(&draft.owner_id, &user_id.to_string())?;

    let summary = RuleDraftSummary {
        id: draft.id.clone(),
        name: draft.name.clone(),
        player_count: draft.player_count,
        description: draft.description.clone(),
        status: draft.status,
        updated_at: draft.updated_at,
        published_rule_id: draft.published_rule_id.clone(),
    };
    let published_rule_id = draft.published_rule_id.clone();

    persistence
        .delete_draft(&draft_id, &user_id.to_string())
        .await?;
    guard.drafts.remove(&draft_id);

    if let Some(rule_id) = published_rule_id {
        guard.published.remove(&rule_id);
    }

    Ok(Json(ApiResponse::success(summary)))
}

/// 作者提交规则草稿进入审核队列。
/// 允许从 Draft / Rejected 进入 PendingReview。
/// Published 状态（已上线）不应直接出现在这里——`update_draft` 已经把任何编辑都拉回 Draft，
/// 所以正常 FE 调用不会触发；但为了防御性，这里仍接受 Published 输入并拉回 PendingReview，
/// 让作者从市场版本"复审"也能走通。
pub async fn submit_review(
    TokenClaims { user_id, .. }: TokenClaims,
    State(store): State<RuleStore>,
    State(persistence): State<RulePersistence>,
    Path(draft_id): Path<String>,
) -> Result<Json<ApiResponse<SaveDraftResponse>>, AppError> {
    // 提交前再 parse 一遍，提前把 design 错误暴露给作者，省得审核员看到一份本就跑不通的规则。
    let mut guard = store.write().await;
    let draft = guard.drafts.get_mut(&draft_id).ok_or(AppError::NotFound)?;
    ensure_owner(&draft.owner_id, &user_id.to_string())?;
    RuleEngine::parse(
        draft.name.clone(),
        draft.player_count,
        draft.description.clone(),
        draft.design.clone(),
    )?;

    if matches!(draft.status, RuleStatus::PendingReview) {
        // 幂等：已在审核队列里就直接返回当前状态，避免 FE 误触发多次时报错。
        let resp = SaveDraftResponse {
            id: draft.id.clone(),
            status: draft.status,
            updated_at: draft.updated_at,
        };
        return Ok(Json(ApiResponse::success(resp)));
    }

    draft.status = RuleStatus::PendingReview;
    draft.reject_reason = None;
    draft.updated_at = now_millis();
    persistence.save_draft(draft).await?;

    Ok(Json(ApiResponse::success(SaveDraftResponse {
        id: draft.id.clone(),
        status: draft.status,
        updated_at: draft.updated_at,
    })))
}

/// 老的 `POST /api/rules/drafts/{id}/publish` 接口——保留路径但行为已经降级为"提交审核"。
/// FE 旧版本仍可继续调用，新版本应改用 `submit-review`。删除路由会让旧 FE 立刻 404，
/// 因此 Phase 1 选择降级别名而非删除。下一个版本可以下线。
#[deprecated(note = "改用 submit_review；本函数保留只为兼容旧 FE，行为已变成提审而非直发")]
pub async fn publish_draft(
    claims: TokenClaims,
    store: State<RuleStore>,
    persistence: State<RulePersistence>,
    path: Path<String>,
) -> Result<Json<ApiResponse<SaveDraftResponse>>, AppError> {
    submit_review(claims, store, persistence, path).await
}

#[derive(Debug, Deserialize)]
pub struct RejectDraftRequest {
    pub reason: String,
}

const REJECT_REASON_MAX_LEN: usize = 512;

/// 校验驳回理由：trim 后非空，长度上限 512 字。
/// 抽出来方便单测覆盖（审核员手动写理由是高频出错点）。
pub fn validate_reject_reason(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput("驳回理由不能为空".to_string()));
    }
    if trimmed.chars().count() > REJECT_REASON_MAX_LEN {
        return Err(AppError::InvalidInput(format!(
            "驳回理由不能超过 {REJECT_REASON_MAX_LEN} 字"
        )));
    }
    Ok(trimmed.to_string())
}

/// 管理员守卫：查 DB 拿当前用户角色，非 admin 返 403。
/// 写成普通 async fn 而非 axum extractor，省掉 FromRequestParts/State 注入的样板代码。
pub async fn ensure_admin(user_id: &UserId, user_repo: &UserRepository) -> Result<(), AppError> {
    let user = user_repo
        .find_by_id(user_id)
        .await?
        .ok_or(AppError::Unauthorized("用户不存在".to_string()))?;
    if user.role != "admin" {
        return Err(AppError::Forbidden("仅管理员可执行此操作".to_string()));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingDraftSummary {
    #[serde(rename = "draftId")]
    pub draft_id: String,
    pub name: String,
    #[serde(rename = "ownerId")]
    pub owner_id: String,
    #[serde(rename = "ownerName")]
    pub owner_name: String,
    #[serde(rename = "playerCount")]
    pub player_count: u8,
    pub description: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    pub design: ExportedRuleDesign,
}

pub async fn list_pending_reviews(
    TokenClaims { user_id, .. }: TokenClaims,
    State(store): State<RuleStore>,
    State(user_repo): State<Arc<UserRepository>>,
) -> Result<Json<ApiResponse<Vec<PendingDraftSummary>>>, AppError> {
    ensure_admin(&user_id, &user_repo).await?;

    // 1. 先快照需要的 owner_id 列表，避免在锁里 await DB。
    let drafts: Vec<RuleDraft> = {
        let guard = store.read().await;
        guard
            .drafts
            .values()
            .filter(|d| d.status == RuleStatus::PendingReview)
            .cloned()
            .collect()
    };

    // 2. 解析作者姓名（缓存避免重复查同一作者）。失败时退化为 owner_id 字符串。
    let mut owner_name_cache: HashMap<String, String> = HashMap::new();
    let mut out = Vec::with_capacity(drafts.len());
    for draft in drafts {
        let owner_name = if let Some(name) = owner_name_cache.get(&draft.owner_id) {
            name.clone()
        } else {
            let resolved = match Uuid::parse_str(&draft.owner_id) {
                Ok(uuid) => user_repo
                    .find_by_id(&UserId(uuid))
                    .await
                    .ok()
                    .flatten()
                    .map(|u| u.name)
                    .unwrap_or_else(|| draft.owner_id.clone()),
                Err(_) => draft.owner_id.clone(),
            };
            owner_name_cache.insert(draft.owner_id.clone(), resolved.clone());
            resolved
        };
        out.push(PendingDraftSummary {
            draft_id: draft.id.clone(),
            name: draft.name.clone(),
            owner_id: draft.owner_id.clone(),
            owner_name,
            player_count: draft.player_count,
            description: draft.description.clone(),
            updated_at: draft.updated_at,
            design: draft.design.clone(),
        });
    }
    // 早提交的排前面，让审核员按 FIFO 处理。
    out.sort_by_key(|d| d.updated_at);
    Ok(Json(ApiResponse::success(out)))
}

pub async fn approve_draft(
    TokenClaims { user_id, .. }: TokenClaims,
    State(store): State<RuleStore>,
    State(persistence): State<RulePersistence>,
    State(user_repo): State<Arc<UserRepository>>,
    Path(draft_id): Path<String>,
) -> Result<Json<ApiResponse<PublishRuleResponse>>, AppError> {
    ensure_admin(&user_id, &user_repo).await?;

    let mut guard = store.write().await;
    let draft = guard.drafts.get_mut(&draft_id).ok_or(AppError::NotFound)?;
    if draft.status != RuleStatus::PendingReview {
        return Err(AppError::Conflict(format!(
            "草稿当前状态为 {}，无法批准，必须先重新提交审核",
            draft.status.to_db_str()
        )));
    }

    // 发布运行态规则，房间开局时按 ruleId 取出再次用于初始化对局。
    let runtime = RuleEngine::parse(
        draft.name.clone(),
        draft.player_count,
        draft.description.clone(),
        draft.design.clone(),
    )?;
    let now = now_millis();
    let rule_id = draft
        .published_rule_id
        .clone()
        .unwrap_or_else(|| format!("rule_{}", uuid::Uuid::new_v4()));

    draft.status = RuleStatus::Published;
    draft.updated_at = now;
    draft.published_rule_id = Some(rule_id.clone());
    draft.reject_reason = None;
    let owner_id_string = draft.owner_id.clone();

    let published_rule = PublishedRule {
        id: rule_id.clone(),
        owner_id: owner_id_string,
        name: draft.name.clone(),
        player_count: draft.player_count,
        description: draft.description.clone(),
        version: 1,
        design: draft.design.clone(),
        runtime,
        created_at: now,
        updated_at: now,
    };

    persistence.save_draft(draft).await?;
    persistence
        .save_published_rule(&published_rule, &draft_id)
        .await?;
    guard.published.insert(rule_id.clone(), published_rule);

    Ok(Json(ApiResponse::success(PublishRuleResponse {
        rule_id,
        version: 1,
        status: RuleStatus::Published,
    })))
}

pub async fn reject_draft(
    TokenClaims { user_id, .. }: TokenClaims,
    State(store): State<RuleStore>,
    State(persistence): State<RulePersistence>,
    State(user_repo): State<Arc<UserRepository>>,
    Path(draft_id): Path<String>,
    Json(payload): Json<RejectDraftRequest>,
) -> Result<Json<ApiResponse<SaveDraftResponse>>, AppError> {
    ensure_admin(&user_id, &user_repo).await?;

    // 先校验入参再加写锁，避免反复加锁。
    let reason = validate_reject_reason(&payload.reason)?;

    let mut guard = store.write().await;
    let draft = guard.drafts.get_mut(&draft_id).ok_or(AppError::NotFound)?;
    if draft.status != RuleStatus::PendingReview {
        return Err(AppError::Conflict(format!(
            "草稿当前状态为 {}，无法驳回，仅 pending_review 可被驳回",
            draft.status.to_db_str()
        )));
    }

    draft.status = RuleStatus::Rejected;
    draft.reject_reason = Some(reason);
    draft.updated_at = now_millis();
    persistence.save_draft(draft).await?;

    Ok(Json(ApiResponse::success(SaveDraftResponse {
        id: draft.id.clone(),
        status: draft.status,
        updated_at: draft.updated_at,
    })))
}

#[derive(Debug, Deserialize)]
pub struct ForkRuleRequest {
    pub name: String,
}

const FORK_NAME_MAX_LEN: usize = 255;

/// 校验 / 兜底 fork 草稿名称：
/// - 去首尾空白
/// - 空 → "{原名} (副本)"
/// - 超长 → 拒
pub fn resolve_fork_name(raw: &str, source_name: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    let name = if trimmed.is_empty() {
        format!("{source_name} (副本)")
    } else {
        trimmed.to_string()
    };
    if name.chars().count() > FORK_NAME_MAX_LEN {
        return Err(AppError::InvalidInput(format!(
            "草稿名称不能超过 {FORK_NAME_MAX_LEN} 字符"
        )));
    }
    Ok(name)
}

/// 把已发布规则的关键字段克隆到一个新的 RuleDraft 上。
/// 抽成纯函数，方便单测覆盖：design 完整复制、forked_from_rule_id 正确。
pub fn build_forked_draft(rule: &PublishedRule, user_id: &Uuid, name: String) -> RuleDraft {
    let now = now_millis();
    RuleDraft {
        id: uuid::Uuid::new_v4().to_string(),
        owner_id: user_id.to_string(),
        name,
        player_count: rule.player_count,
        description: rule.description.clone(),
        status: RuleStatus::Draft,
        design: rule.design.clone(),
        created_at: now,
        updated_at: now,
        published_rule_id: None,
        forked_from_rule_id: Some(rule.id.clone()),
        reject_reason: None,
    }
}

pub async fn fork_published_rule(
    TokenClaims { user_id, .. }: TokenClaims,
    State(store): State<RuleStore>,
    State(persistence): State<RulePersistence>,
    Path(rule_id): Path<String>,
    Json(payload): Json<ForkRuleRequest>,
) -> Result<Json<ApiResponse<SaveDraftResponse>>, AppError> {
    // 1. 拿到源规则，复制必要字段后立刻释放读锁，避免 save_draft 期间长时间占用。
    let source = {
        let guard = store.read().await;
        guard
            .published
            .get(&rule_id)
            .cloned()
            .ok_or(AppError::NotFound)?
    };

    // 2. 校验/兜底名称。
    let name = resolve_fork_name(&payload.name, &source.name)?;

    let draft = build_forked_draft(&source, &user_id.0, name);
    let response = SaveDraftResponse {
        id: draft.id.clone(),
        status: draft.status,
        updated_at: draft.updated_at,
    };

    persistence.save_draft(&draft).await?;
    store.write().await.drafts.insert(draft.id.clone(), draft);

    Ok(Json(ApiResponse::success(response)))
}

pub async fn rule_options(
    State(store): State<RuleStore>,
) -> Result<Json<ApiResponse<Vec<RuleOption>>>, AppError> {
    let guard = store.read().await;
    let mut options = guard
        .published
        .values()
        .map(|rule| RuleOption {
            id: rule.id.clone(),
            name: rule.name.clone(),
            player_count: rule.player_count,
            description: rule.description.clone(),
        })
        .collect::<Vec<_>>();

    options.sort_by(|left, right| {
        builtin_rule_sort_key(&left.id)
            .cmp(&builtin_rule_sort_key(&right.id))
            .then_with(|| left.name.cmp(&right.name))
    });

    Ok(Json(ApiResponse::success(options)))
}

impl RulePersistence {
    pub async fn ensure_schema(&self) -> Result<(), AppError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS rule_drafts (
                id UUID PRIMARY KEY,
                owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                name VARCHAR(255) NOT NULL,
                player_count SMALLINT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                status VARCHAR(32) NOT NULL DEFAULT 'draft',
                design JSONB NOT NULL,
                published_rule_id VARCHAR(128),
                forked_from_rule_id VARCHAR(128),
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        // 老库升级：保证 fork 字段存在（Postgres 9.6+ 支持 IF NOT EXISTS）。
        sqlx::query(
            r#"
            ALTER TABLE rule_drafts
                ADD COLUMN IF NOT EXISTS forked_from_rule_id VARCHAR(128)
            "#,
        )
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        // 审核流引入的驳回理由字段，老库升级用 IF NOT EXISTS 保持幂等。
        sqlx::query(
            r#"
            ALTER TABLE rule_drafts
                ADD COLUMN IF NOT EXISTS reject_reason TEXT
            "#,
        )
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        // 审核员权限：users 表加 role 字段，默认 'user'，幂等升级。
        sqlx::query(
            r#"
            ALTER TABLE users
                ADD COLUMN IF NOT EXISTS role VARCHAR(16) NOT NULL DEFAULT 'user'
            "#,
        )
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        // 首任管理员 boot-strap：仅当 Tanhhhhtjy 当前不是 admin 时才更新，
        // 避免覆盖运维手动改过的角色（例如临时降级）。这条 UPDATE 在用户表为空时是 no-op。
        sqlx::query(
            r#"
            UPDATE users
                SET role = 'admin'
                WHERE name = 'Tanhhhhtjy' AND role <> 'admin'
            "#,
        )
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_rule_drafts_owner_updated
                ON rule_drafts(owner_id, updated_at DESC)
            "#,
        )
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS rule_published (
                id UUID PRIMARY KEY,
                draft_id UUID REFERENCES rule_drafts(id) ON DELETE SET NULL,
                owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                name VARCHAR(255) NOT NULL,
                player_count SMALLINT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                version INTEGER NOT NULL DEFAULT 1,
                design JSONB NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_rule_published_owner_updated
                ON rule_published(owner_id, updated_at DESC)
            "#,
        )
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        Ok(())
    }

    pub async fn load_into(&self, repository: &mut RuleRepository) -> Result<(), AppError> {
        let draft_rows = sqlx::query(
            r#"
            SELECT
                id::text AS id,
                owner_id,
                name,
                player_count,
                description,
                status,
                design,
                published_rule_id,
                forked_from_rule_id,
                reject_reason,
                (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_at,
                (EXTRACT(EPOCH FROM updated_at) * 1000)::bigint AS updated_at
            FROM rule_drafts
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        for row in draft_rows {
            let design: serde_json::Value = row.get("design");
            let design: ExportedRuleDesign =
                serde_json::from_value(design).map_err(AppError::JsonError)?;
            let status = RuleStatus::from_db_str(row.get::<String, _>("status").as_str());

            let draft = RuleDraft {
                id: row.get("id"),
                owner_id: row.get::<Uuid, _>("owner_id").to_string(),
                name: row.get("name"),
                player_count: row.get::<i16, _>("player_count") as u8,
                description: row.get("description"),
                status,
                design,
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                published_rule_id: row.get("published_rule_id"),
                forked_from_rule_id: row.get("forked_from_rule_id"),
                reject_reason: row.get("reject_reason"),
            };
            repository.drafts.insert(draft.id.clone(), draft);
        }

        let published_rows = sqlx::query(
            r#"
            SELECT
                id::text AS id,
                owner_id,
                name,
                player_count,
                description,
                version,
                design,
                (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_at,
                (EXTRACT(EPOCH FROM updated_at) * 1000)::bigint AS updated_at
            FROM rule_published
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        for row in published_rows {
            let design: serde_json::Value = row.get("design");
            let design: ExportedRuleDesign =
                serde_json::from_value(design).map_err(AppError::JsonError)?;
            let name: String = row.get("name");
            let player_count = row.get::<i16, _>("player_count") as u8;
            let description: String = row.get("description");
            let runtime = RuleEngine::parse(
                name.clone(),
                player_count,
                description.clone(),
                design.clone(),
            )?;
            // 历史数据里 rule_published.id 存纯 UUID，但内存 / API 用 "rule_<uuid>" 前缀，
            // 与 rule_drafts.published_rule_id 字面值对齐。
            let id_raw: String = row.get("id");
            let published_rule = PublishedRule {
                id: format!("rule_{id_raw}"),
                owner_id: row.get::<Uuid, _>("owner_id").to_string(),
                name,
                player_count,
                description,
                version: row.get::<i32, _>("version") as u32,
                design,
                runtime,
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            };
            repository
                .published
                .insert(published_rule.id.clone(), published_rule);
        }

        Ok(())
    }

    pub async fn save_draft(&self, draft: &RuleDraft) -> Result<(), AppError> {
        let design = serde_json::to_value(&draft.design).map_err(AppError::JsonError)?;
        let owner_id = Uuid::parse_str(&draft.owner_id)
            .map_err(|e| AppError::InvalidInput(format!("规则作者 ID 无效：{e}")))?;
        let draft_uuid = Uuid::parse_str(&draft.id)
            .map_err(|e| AppError::InvalidInput(format!("草稿 ID 必须是 UUID：{e}")))?;
        let status = draft.status.to_db_str();

        sqlx::query(
            r#"
            INSERT INTO rule_drafts (
                id, owner_id, name, player_count, description, status, design,
                published_rule_id, forked_from_rule_id, reject_reason, created_at, updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                to_timestamp($11::double precision / 1000.0),
                to_timestamp($12::double precision / 1000.0)
            )
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                player_count = EXCLUDED.player_count,
                description = EXCLUDED.description,
                status = EXCLUDED.status,
                design = EXCLUDED.design,
                published_rule_id = EXCLUDED.published_rule_id,
                forked_from_rule_id = EXCLUDED.forked_from_rule_id,
                reject_reason = EXCLUDED.reject_reason,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(draft_uuid)
        .bind(owner_id)
        .bind(&draft.name)
        .bind(draft.player_count as i16)
        .bind(&draft.description)
        .bind(status)
        .bind(design)
        .bind(&draft.published_rule_id)
        .bind(&draft.forked_from_rule_id)
        .bind(&draft.reject_reason)
        .bind(draft.created_at)
        .bind(draft.updated_at)
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        Ok(())
    }

    pub async fn save_published_rule(
        &self,
        rule: &PublishedRule,
        draft_id: &str,
    ) -> Result<(), AppError> {
        let design = serde_json::to_value(&rule.design).map_err(AppError::JsonError)?;
        let owner_id = Uuid::parse_str(&rule.owner_id)
            .map_err(|e| AppError::InvalidInput(format!("规则作者 ID 无效：{e}")))?;
        // 内存 / API 用 "rule_<uuid>" 前缀，落库时剥掉只留 UUID。
        let rule_uuid_str = rule.id.strip_prefix("rule_").unwrap_or(&rule.id);
        let rule_uuid = Uuid::parse_str(rule_uuid_str).map_err(|e| {
            AppError::InvalidInput(format!("已发布规则 ID 必须是 UUID（含 rule_ 前缀）：{e}"))
        })?;
        let draft_uuid = Uuid::parse_str(draft_id)
            .map_err(|e| AppError::InvalidInput(format!("草稿 ID 必须是 UUID：{e}")))?;

        sqlx::query(
            r#"
            INSERT INTO rule_published (
                id, draft_id, owner_id, name, player_count, description, version,
                design, created_at, updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8,
                to_timestamp($9::double precision / 1000.0),
                to_timestamp($10::double precision / 1000.0)
            )
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                player_count = EXCLUDED.player_count,
                description = EXCLUDED.description,
                version = rule_published.version + 1,
                design = EXCLUDED.design,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(rule_uuid)
        .bind(draft_uuid)
        .bind(owner_id)
        .bind(&rule.name)
        .bind(rule.player_count as i16)
        .bind(&rule.description)
        .bind(rule.version as i32)
        .bind(design)
        .bind(rule.created_at)
        .bind(rule.updated_at)
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        Ok(())
    }

    pub async fn delete_draft(&self, draft_id: &str, owner_id: &str) -> Result<(), AppError> {
        let owner_id = Uuid::parse_str(owner_id)
            .map_err(|e| AppError::InvalidInput(format!("规则作者 ID 无效：{e}")))?;
        let draft_uuid = Uuid::parse_str(draft_id)
            .map_err(|e| AppError::InvalidInput(format!("草稿 ID 必须是 UUID：{e}")))?;

        sqlx::query(
            r#"
            DELETE FROM rule_published
            WHERE draft_id = $1 AND owner_id = $2
            "#,
        )
        .bind(draft_uuid)
        .bind(owner_id)
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        sqlx::query(
            r#"
            DELETE FROM rule_drafts
            WHERE id = $1 AND owner_id = $2
            "#,
        )
        .bind(draft_uuid)
        .bind(owner_id)
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        Ok(())
    }
}

pub async fn build_rule_store(pool: &PgPool) -> Result<Arc<RwLock<RuleRepository>>, AppError> {
    let mut repository = RuleRepository::default();
    for default_rule in build_builtin_rules() {
        repository
            .published
            .insert(default_rule.id.clone(), default_rule);
    }

    RulePersistence { pool: pool.clone() }
        .ensure_schema()
        .await?;
    RulePersistence { pool: pool.clone() }
        .load_into(&mut repository)
        .await?;

    Ok(Arc::new(RwLock::new(repository)))
}

fn ensure_owner(owner_id: &str, user_id: &str) -> Result<(), AppError> {
    if owner_id == user_id {
        return Ok(());
    }

    Err(AppError::Unauthorized("无权操作该规则".to_string()))
}

fn now_millis() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp_nanos() as i64 / 1_000_000
}

#[allow(dead_code)]
fn build_builtin_test_rule() -> Option<PublishedRule> {
    let rule_path = concat!(env!("CARGO_MANIFEST_DIR"), "\\test.json");
    let content = std::fs::read_to_string(rule_path).ok()?;
    let design: ExportedRuleDesign = serde_json::from_str(&content).ok()?;
    let runtime = RuleEngine::parse(
        "测试规则".to_string(),
        2,
        "基于根目录 test.json 预置的可联调规则".to_string(),
        design.clone(),
    )
    .ok()?;
    let now = now_millis();

    Some(PublishedRule {
        id: "builtin-test-rule".to_string(),
        owner_id: "system".to_string(),
        name: "测试规则".to_string(),
        player_count: 2,
        description: "基于根目录 test.json 预置的可联调规则".to_string(),
        version: 1,
        design,
        runtime,
        created_at: now,
        updated_at: now,
    })
}

struct BuiltinRuleSpec {
    id: &'static str,
    name: &'static str,
    player_count: u8,
    description: &'static str,
    design_file: &'static str,
}

const BUILTIN_RULES: &[BuiltinRuleSpec] = &[
    BuiltinRuleSpec {
        id: "builtin-test2-rule",
        name: "Tiny Demo",
        player_count: 2,
        description: "Minimal playable builtin rule loaded from test2.json.",
        design_file: "test2.json",
    },
    BuiltinRuleSpec {
        id: "builtin-test-rule",
        name: "Duel Demo",
        player_count: 2,
        description: "Playable builtin rule loaded from test.json.",
        design_file: "test.json",
    },
    BuiltinRuleSpec {
        id: "classic",
        name: "Classic Demo",
        player_count: 2,
        description: "Legacy room rule kept for compatibility. Uses the same duel flow as test.json.",
        design_file: "test.json",
    },
    BuiltinRuleSpec {
        id: "party",
        name: "Party Demo",
        player_count: 2,
        description: "Legacy room rule kept for compatibility. Uses the same duel flow as test.json.",
        design_file: "test.json",
    },
    BuiltinRuleSpec {
        id: "builtin-war-rule",
        name: "War 拼点战争",
        player_count: 2,
        description: "经典战争玩法：每人 5 张手牌，连续 5 轮翻牌比大小，赢得轮数多者胜。",
        design_file: "war.json",
    },
    BuiltinRuleSpec {
        id: "builtin-99-rule",
        name: "99 累加",
        player_count: 2,
        description: "经典累加玩法：每人 14 张牌，轮流出牌把点数加进桌面总和；让总和超过 99 的玩家输。",
        design_file: "nine_nine.json",
    },
    BuiltinRuleSpec {
        id: "builtin-bigtwo-rule",
        name: "大老二极简版",
        player_count: 2,
        description: "简化版大老二：单/对/三 + 炸弹，先出完赢。注意：极简版规则下，双方均无法压制当前牌型时可能进入死锁，建议尝试单张或小对子开局以保留出牌空间。",
        design_file: "big_two.json",
    },
    BuiltinRuleSpec {
        id: "builtin-blackjack-rule",
        name: "21 点（伪版）",
        player_count: 2,
        description: "伪版 21 点：无要牌/停牌阶段，每方一次性提交 3 张明牌后直接比较结果。A=1，J/Q/K 按字面 11/12/13 算；谁的总和最接近 21 但不超过谁赢，双方均爆或平局判和。",
        design_file: "blackjack.json",
    },
];

fn build_builtin_rules() -> Vec<PublishedRule> {
    let mut cache: HashMap<&'static str, ExportedRuleDesign> = HashMap::new();
    let mut rules = Vec::new();

    for spec in BUILTIN_RULES {
        let design = match cache.get(spec.design_file) {
            Some(design) => design.clone(),
            None => match load_builtin_design(spec.design_file) {
                Some(design) => {
                    cache.insert(spec.design_file, design.clone());
                    design
                }
                None => continue,
            },
        };
        if let Some(rule) = build_builtin_rule(
            spec.id,
            spec.name,
            spec.player_count,
            spec.description,
            design,
        ) {
            rules.push(rule);
        }
    }
    rules
}

fn load_builtin_design(filename: &str) -> Option<ExportedRuleDesign> {
    let rule_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(filename);
    let content = std::fs::read_to_string(rule_path).ok()?;
    serde_json::from_str(&content).ok()
}

fn build_builtin_rule(
    id: &str,
    name: &str,
    player_count: u8,
    description: &str,
    design: ExportedRuleDesign,
) -> Option<PublishedRule> {
    let runtime = RuleEngine::parse(
        name.to_string(),
        player_count,
        description.to_string(),
        design.clone(),
    )
    .ok()?;
    let now = now_millis();

    Some(PublishedRule {
        id: id.to_string(),
        owner_id: "system".to_string(),
        name: name.to_string(),
        player_count,
        description: description.to_string(),
        version: 1,
        design,
        runtime,
        created_at: now,
        updated_at: now,
    })
}

fn builtin_rule_sort_key(rule_id: &str) -> (u8, &str) {
    let order = BUILTIN_RULES
        .iter()
        .position(|spec| spec.id == rule_id)
        .map(|idx| idx as u8)
        .unwrap_or(u8::MAX);
    (order, rule_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_published_rule(id: &str, name: &str) -> PublishedRule {
        // 复用 builtin 资产里第一份能跑通的设计，保证 design 完整且能解析。
        let builtins = build_builtin_rules();
        let source = builtins
            .into_iter()
            .next()
            .expect("至少需要一份内置规则才能跑这条测试（test2.json）");
        PublishedRule {
            id: id.to_string(),
            owner_id: "system".to_string(),
            name: name.to_string(),
            player_count: source.player_count,
            description: "fork 测试用的源规则".to_string(),
            version: 1,
            design: source.design.clone(),
            runtime: source.runtime,
            created_at: source.created_at,
            updated_at: source.updated_at,
        }
    }

    #[test]
    fn fork_request_deserializes_minimal_payload() {
        let req: ForkRuleRequest = serde_json::from_str(r#"{"name":"我的副本"}"#).unwrap();
        assert_eq!(req.name, "我的副本");
    }

    #[test]
    fn build_forked_draft_copies_design_and_records_source() {
        let rule = sample_published_rule("builtin-war-rule", "War 拼点战争");
        let user = Uuid::new_v4();

        let draft = build_forked_draft(&rule, &user, "我的战争副本".to_string());

        assert_eq!(draft.name, "我的战争副本");
        assert_eq!(draft.owner_id, user.to_string());
        assert_eq!(draft.player_count, rule.player_count);
        assert_eq!(draft.description, rule.description);
        assert!(matches!(draft.status, RuleStatus::Draft));
        assert_eq!(draft.published_rule_id, None);
        assert_eq!(
            draft.forked_from_rule_id.as_deref(),
            Some("builtin-war-rule")
        );
        // design 必须是完整克隆而不是丢字段——用 serde_json 等价对比最稳。
        let original_json = serde_json::to_value(&rule.design).unwrap();
        let forked_json = serde_json::to_value(&draft.design).unwrap();
        assert_eq!(original_json, forked_json);
    }

    #[test]
    fn build_forked_draft_preserves_user_uuid_with_uuid_rule_id() {
        // rule_id 是 "rule_<uuid>" 风格时也要原样保留，不做 UUID 转换。
        let mut rule = sample_published_rule("rule_abc123", "用户规则");
        rule.id = "rule_550e8400-e29b-41d4-a716-446655440000".to_string();
        let user = Uuid::new_v4();

        let draft = build_forked_draft(&rule, &user, "副本".to_string());

        assert_eq!(
            draft.forked_from_rule_id.as_deref(),
            Some("rule_550e8400-e29b-41d4-a716-446655440000")
        );
    }

    #[test]
    fn resolve_fork_name_falls_back_to_copy_suffix_when_empty() {
        assert_eq!(
            resolve_fork_name("", "War 拼点战争").unwrap(),
            "War 拼点战争 (副本)"
        );
        assert_eq!(
            resolve_fork_name("   ", "Tiny Demo").unwrap(),
            "Tiny Demo (副本)"
        );
    }

    #[test]
    fn resolve_fork_name_trims_user_input() {
        assert_eq!(resolve_fork_name("  我的副本  ", "源").unwrap(), "我的副本");
    }

    #[test]
    fn resolve_fork_name_rejects_overlong_name() {
        let too_long: String = "字".repeat(FORK_NAME_MAX_LEN + 1);
        let err = resolve_fork_name(&too_long, "源").unwrap_err();
        assert!(
            matches!(err, AppError::InvalidInput(ref msg) if msg.contains(&FORK_NAME_MAX_LEN.to_string())),
            "expected InvalidInput mentioning the limit, got {err:?}"
        );
    }

    // ----- 审核流相关 -----

    #[test]
    fn rule_status_serializes_as_camel_case() {
        // 出网（FE / JSON）协议：四态都用 camelCase。
        assert_eq!(
            serde_json::to_string(&RuleStatus::Draft).unwrap(),
            "\"draft\""
        );
        assert_eq!(
            serde_json::to_string(&RuleStatus::PendingReview).unwrap(),
            "\"pendingReview\""
        );
        assert_eq!(
            serde_json::to_string(&RuleStatus::Published).unwrap(),
            "\"published\""
        );
        assert_eq!(
            serde_json::to_string(&RuleStatus::Rejected).unwrap(),
            "\"rejected\""
        );
    }

    #[test]
    fn rule_status_deserializes_from_camel_case() {
        assert_eq!(
            serde_json::from_str::<RuleStatus>("\"pendingReview\"").unwrap(),
            RuleStatus::PendingReview
        );
        assert_eq!(
            serde_json::from_str::<RuleStatus>("\"rejected\"").unwrap(),
            RuleStatus::Rejected
        );
    }

    #[test]
    fn rule_status_db_string_roundtrip_all_variants() {
        // DB 层用 snake_case，与历史数据保持一致；四态都要能往返。
        for status in [
            RuleStatus::Draft,
            RuleStatus::PendingReview,
            RuleStatus::Published,
            RuleStatus::Rejected,
        ] {
            let s = status.to_db_str();
            assert_eq!(RuleStatus::from_db_str(s), status, "roundtrip 失败：{s}");
        }
    }

    #[test]
    fn rule_status_from_db_str_unknown_falls_back_to_draft() {
        // 兜底：未知字符串（脏数据 / 旧版本残留）按 Draft 处理，避免阻塞读取。
        assert_eq!(RuleStatus::from_db_str(""), RuleStatus::Draft);
        assert_eq!(RuleStatus::from_db_str("garbage"), RuleStatus::Draft);
        // camelCase 写入 DB 也算异常，兜底成 Draft（理论上不会发生，但要稳）。
        assert_eq!(RuleStatus::from_db_str("pendingReview"), RuleStatus::Draft);
    }

    #[test]
    fn validate_reject_reason_rejects_empty_and_whitespace() {
        assert!(matches!(
            validate_reject_reason(""),
            Err(AppError::InvalidInput(_))
        ));
        assert!(matches!(
            validate_reject_reason("   \n\t  "),
            Err(AppError::InvalidInput(_))
        ));
    }

    #[test]
    fn validate_reject_reason_trims_and_keeps_content() {
        assert_eq!(
            validate_reject_reason("  规则名称疑似违规  ").unwrap(),
            "规则名称疑似违规"
        );
    }

    #[test]
    fn validate_reject_reason_rejects_overlong_text() {
        let too_long: String = "字".repeat(REJECT_REASON_MAX_LEN + 1);
        let err = validate_reject_reason(&too_long).unwrap_err();
        assert!(
            matches!(err, AppError::InvalidInput(ref msg) if msg.contains(&REJECT_REASON_MAX_LEN.to_string())),
            "expected InvalidInput mentioning the limit, got {err:?}"
        );
    }

    #[test]
    fn validate_reject_reason_allows_exactly_max_len() {
        // 边界值：恰好 512 字应通过。
        let exact: String = "x".repeat(REJECT_REASON_MAX_LEN);
        assert_eq!(validate_reject_reason(&exact).unwrap().len(), exact.len());
    }

    /// 模拟 update_draft 的状态重置逻辑：作者编辑非 Draft 草稿应被拉回 Draft 并清空 reject_reason。
    /// 把核心状态机抽出来用纯函数测，不需要起 DB / store。
    fn apply_edit_reset(
        status: RuleStatus,
        reject_reason: Option<String>,
    ) -> (RuleStatus, Option<String>) {
        if status != RuleStatus::Draft {
            (RuleStatus::Draft, None)
        } else {
            (status, reject_reason)
        }
    }

    #[test]
    fn edit_resets_pending_review_to_draft() {
        let (status, reason) = apply_edit_reset(RuleStatus::PendingReview, None);
        assert_eq!(status, RuleStatus::Draft);
        assert!(reason.is_none());
    }

    #[test]
    fn edit_resets_rejected_to_draft_and_clears_reason() {
        let (status, reason) = apply_edit_reset(RuleStatus::Rejected, Some("不合规".to_string()));
        assert_eq!(status, RuleStatus::Draft);
        assert!(reason.is_none(), "rejected 编辑后必须清掉 reject_reason");
    }

    #[test]
    fn edit_resets_published_to_draft_keeping_market_version_untouched() {
        // 已发布草稿被作者再编辑，本地状态拉回 Draft；rule_published 行不在本函数职责内。
        let (status, _) = apply_edit_reset(RuleStatus::Published, None);
        assert_eq!(status, RuleStatus::Draft);
    }

    #[test]
    fn edit_keeps_draft_status_intact() {
        let (status, reason) = apply_edit_reset(RuleStatus::Draft, None);
        assert_eq!(status, RuleStatus::Draft);
        assert!(reason.is_none());
    }

    #[test]
    fn reject_draft_request_deserializes_camel_case_reason() {
        let req: RejectDraftRequest = serde_json::from_str(r#"{"reason":"重复规则"}"#).unwrap();
        assert_eq!(req.reason, "重复规则");
    }

    #[test]
    fn rule_draft_serializes_reject_reason_only_when_present() {
        // None 时字段不出现在 JSON 里，避免 FE 模板空字段触发"显示驳回理由"逻辑。
        let mut draft = RuleDraft {
            id: Uuid::new_v4().to_string(),
            owner_id: Uuid::new_v4().to_string(),
            name: "demo".to_string(),
            player_count: 2,
            description: String::new(),
            status: RuleStatus::Draft,
            design: serde_json::from_value(serde_json::json!({
                "cards": [],
                "groups": [],
                "components": [],
                "stateMachine": {"initial":"s","states":{}},
                "rule": {"playerCount":2}
            }))
            .unwrap_or_else(|_| sample_published_rule("x", "x").design),
            created_at: 0,
            updated_at: 0,
            published_rule_id: None,
            forked_from_rule_id: None,
            reject_reason: None,
        };
        let json_no_reason = serde_json::to_value(&draft).unwrap();
        assert!(
            json_no_reason.get("rejectReason").is_none(),
            "reject_reason=None 时不应输出 rejectReason 字段"
        );

        draft.reject_reason = Some("规则不完整".to_string());
        let json_with_reason = serde_json::to_value(&draft).unwrap();
        assert_eq!(
            json_with_reason
                .get("rejectReason")
                .and_then(|v| v.as_str()),
            Some("规则不完整")
        );
    }
}
