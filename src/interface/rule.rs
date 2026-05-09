use std::{collections::HashMap, sync::Arc};

use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

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

pub async fn save_draft(
    TokenClaims { user_id, .. }: TokenClaims,
    State(store): State<RuleStore>,
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

    store.write().await.drafts.insert(draft.id.clone(), draft);
    Ok(Json(ApiResponse::success(response)))
}

pub async fn update_draft(
    TokenClaims { user_id, .. }: TokenClaims,
    State(store): State<RuleStore>,
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

pub async fn publish_draft(
    TokenClaims { user_id, .. }: TokenClaims,
    State(store): State<RuleStore>,
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
    let options = guard
        .published
        .values()
        .map(|rule| RuleOption {
            id: rule.id.clone(),
            name: rule.name.clone(),
            player_count: rule.player_count,
            description: rule.description.clone(),
        })
        .collect();

    Ok(Json(ApiResponse::success(options)))
}

pub fn build_rule_store() -> Arc<RwLock<RuleRepository>> {
    Arc::new(RwLock::new(RuleRepository::default()))
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
