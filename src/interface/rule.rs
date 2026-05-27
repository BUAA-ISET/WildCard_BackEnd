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
    domain::rule_engine::{ExportedRuleDesign, RuleEngine, RuntimeRule},
    error::AppError,
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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleStatus {
    Draft,
    Published,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublishedRule {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub player_count: u8,
    pub description: String,
    pub version: u32,
    pub design: ExportedRuleDesign,
    #[serde(skip_serializing)]
    pub runtime: RuntimeRule,
    pub created_at: i64,
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
            status: draft.status.clone(),
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
        id: format!("rule_draft_{}", uuid::Uuid::new_v4()),
        owner_id: user_id.to_string(),
        name: payload.name,
        player_count: payload.player_count,
        description: payload.description,
        status: RuleStatus::Draft,
        design: payload.design,
        created_at: now,
        updated_at: now,
        published_rule_id: None,
    };
    let response = SaveDraftResponse {
        id: draft.id.clone(),
        status: draft.status.clone(),
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
    draft.status = RuleStatus::Draft;
    draft.updated_at = now;
    persistence.save_draft(draft).await?;

    Ok(Json(ApiResponse::success(SaveDraftResponse {
        id: draft.id.clone(),
        status: draft.status.clone(),
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
        status: draft.status.clone(),
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

pub async fn publish_draft(
    TokenClaims { user_id, .. }: TokenClaims,
    State(store): State<RuleStore>,
    State(persistence): State<RulePersistence>,
    Path(draft_id): Path<String>,
) -> Result<Json<ApiResponse<PublishRuleResponse>>, AppError> {
    let mut guard = store.write().await;
    let draft = guard.drafts.get_mut(&draft_id).ok_or(AppError::NotFound)?;
    ensure_owner(&draft.owner_id, &user_id.to_string())?;

    // 发布时生成后端运行态规则，房间开局时直接按 ruleId 取出并再次用于初始化对局。
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

    let published_rule = {
        draft.status = RuleStatus::Published;
        draft.updated_at = now;
        draft.published_rule_id = Some(rule_id.clone());

        PublishedRule {
            id: rule_id.clone(),
            owner_id: user_id.to_string(),
            name: draft.name.clone(),
            player_count: draft.player_count,
            description: draft.description.clone(),
            version: 1,
            design: draft.design.clone(),
            runtime,
            created_at: now,
            updated_at: now,
        }
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
        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS rule_drafts (
                id VARCHAR(128) PRIMARY KEY,
                owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                name VARCHAR(255) NOT NULL,
                player_count SMALLINT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                status VARCHAR(32) NOT NULL DEFAULT 'draft',
                design JSONB NOT NULL,
                published_rule_id VARCHAR(128),
                created_at BIGINT NOT NULL,
                updated_at BIGINT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        sqlx::query!(
            r#"
            CREATE INDEX IF NOT EXISTS idx_rule_drafts_owner_updated
                ON rule_drafts(owner_id, updated_at DESC)
            "#,
        )
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS rule_published (
                id VARCHAR(128) PRIMARY KEY,
                draft_id VARCHAR(128) REFERENCES rule_drafts(id) ON DELETE SET NULL,
                owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                name VARCHAR(255) NOT NULL,
                player_count SMALLINT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                version INTEGER NOT NULL DEFAULT 1,
                design JSONB NOT NULL,
                created_at BIGINT NOT NULL,
                updated_at BIGINT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        sqlx::query!(
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
        let draft_rows = sqlx::query!(
            r#"
            SELECT id, owner_id, name, player_count, description, status, design,
                   published_rule_id, created_at, updated_at
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
            let status = match row.get::<String, _>("status").as_str() {
                "published" => RuleStatus::Published,
                _ => RuleStatus::Draft,
            };

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
            };
            repository.drafts.insert(draft.id.clone(), draft);
        }

        let published_rows = sqlx::query!(
            r#"
            SELECT id, owner_id, name, player_count, description, version, design,
                   created_at, updated_at
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
            let published_rule = PublishedRule {
                id: row.get("id"),
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
        let status = match draft.status {
            RuleStatus::Draft => "draft",
            RuleStatus::Published => "published",
        };

        sqlx::query!(
            r#"
            INSERT INTO rule_drafts (
                id, owner_id, name, player_count, description, status, design,
                published_rule_id, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                player_count = EXCLUDED.player_count,
                description = EXCLUDED.description,
                status = EXCLUDED.status,
                design = EXCLUDED.design,
                published_rule_id = EXCLUDED.published_rule_id,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(&draft.id)
        .bind(owner_id)
        .bind(&draft.name)
        .bind(draft.player_count as i16)
        .bind(&draft.description)
        .bind(status)
        .bind(design)
        .bind(&draft.published_rule_id)
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

        sqlx::query!(
            r#"
            INSERT INTO rule_published (
                id, draft_id, owner_id, name, player_count, description, version,
                design, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                player_count = EXCLUDED.player_count,
                description = EXCLUDED.description,
                version = rule_published.version + 1,
                design = EXCLUDED.design,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(&rule.id)
        .bind(draft_id)
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

        sqlx::query!(
            r#"
            DELETE FROM rule_published
            WHERE draft_id = $1 AND owner_id = $2
            "#,
        )
        .bind(draft_id)
        .bind(owner_id)
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        sqlx::query!(
            r#"
            DELETE FROM rule_drafts
            WHERE id = $1 AND owner_id = $2
            "#,
        )
        .bind(draft_id)
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

fn build_builtin_rules() -> Vec<PublishedRule> {
    let mut rules = Vec::new();

    if let Some(design) = load_builtin_test2_design() {
        rules.extend(
            [(
                "builtin-test2-rule",
                "Tiny Demo",
                2,
                "Minimal playable builtin rule loaded from test2.json.",
            )]
            .into_iter()
            .filter_map(|(id, name, player_count, description)| {
                build_builtin_rule(id, name, player_count, description, design.clone())
            }),
        );
    }

    if let Some(design) = load_builtin_test_design() {
        rules.extend(
            [
                (
                    "builtin-test-rule",
                    "Duel Demo",
                    2,
                    "Playable builtin rule loaded from test.json.",
                ),
                (
                    "classic",
                    "Classic Demo",
                    2,
                    "Legacy room rule kept for compatibility. Uses the same duel flow as test.json.",
                ),
                (
                    "party",
                    "Party Demo",
                    2,
                    "Legacy room rule kept for compatibility. Uses the same duel flow as test.json.",
                ),
            ]
            .into_iter()
            .filter_map(|(id, name, player_count, description)| {
                build_builtin_rule(id, name, player_count, description, design.clone())
            }),
        );
    }

    rules
}

fn load_builtin_test_design() -> Option<ExportedRuleDesign> {
    let rule_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test.json");
    let content = std::fs::read_to_string(rule_path).ok()?;
    serde_json::from_str(&content).ok()
}

fn load_builtin_test2_design() -> Option<ExportedRuleDesign> {
    let rule_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test2.json");
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
    match rule_id {
        "builtin-test2-rule" => (0, rule_id),
        "builtin-test-rule" => (1, rule_id),
        "classic" => (2, rule_id),
        "party" => (3, rule_id),
        _ => (4, rule_id),
    }
}
