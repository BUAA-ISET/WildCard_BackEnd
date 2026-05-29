use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::infrastructure::user::UserRepository;
use crate::interface::rule::ApiResponse;
use crate::state::{RoomStore, RuleStore};

#[derive(Debug, Clone, Serialize)]
pub struct MarketDeveloper {
    pub id: String,
    pub name: String,
    pub avatar: String,
}

#[derive(Debug, Serialize)]
pub struct PublishedRuleSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub rule_type: String,
    pub developer: MarketDeveloper,
    pub rating: f32,
    #[serde(rename = "reviewCount")]
    pub review_count: u32,
    #[serde(rename = "publishedAt")]
    pub published_at: i64,
    #[serde(rename = "coverUrl", skip_serializing_if = "Option::is_none")]
    pub cover_url: Option<String>,
    #[serde(rename = "playerCount")]
    pub player_count: u8,
}

#[derive(Debug, Serialize)]
pub struct PublishedRuleDetail {
    #[serde(flatten)]
    pub summary: PublishedRuleSummary,
    pub introduction: String,
    pub screenshots: Vec<String>,
    pub reviews: Vec<RuleReview>,
}

#[derive(Debug, Serialize)]
pub struct RuleReview {
    pub id: String,
    #[serde(rename = "authorName")]
    pub author_name: String,
    #[serde(rename = "authorAvatar")]
    pub author_avatar: String,
    pub rating: u8,
    pub content: String,
    #[serde(rename = "imageUrl", skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct MarketRoomSummary {
    pub id: String,
    pub code: String,
    #[serde(rename = "hostName")]
    pub host_name: String,
    #[serde(rename = "currentPlayers")]
    pub current_players: usize,
    #[serde(rename = "maxPlayers")]
    pub max_players: usize,
    #[serde(rename = "hasPassword")]
    pub has_password: bool,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct RuleQueryParams {
    #[serde(default)]
    pub keyword: Option<String>,
    #[serde(default, rename = "type")]
    pub rule_type: Option<String>,
}

const DEFAULT_RULE_TYPE: &str = "对战";

fn placeholder_introduction(description: &str) -> String {
    if description.is_empty() {
        "作者还没有填写详细介绍。".to_string()
    } else {
        description.to_string()
    }
}

async fn resolve_developer(user_repo: &UserRepository, owner_id: &str) -> MarketDeveloper {
    if let Ok(uuid) = uuid::Uuid::parse_str(owner_id)
        && let Ok(Some(user)) = user_repo
            .find_by_id(&crate::domain::user::UserId(uuid))
            .await
    {
        return MarketDeveloper {
            id: owner_id.to_string(),
            name: user.name,
            avatar: user.avatar,
        };
    }
    MarketDeveloper {
        id: owner_id.to_string(),
        name: "WildCard 内置".to_string(),
        avatar: String::new(),
    }
}

fn matches_filter(
    name: &str,
    description: &str,
    rule_type: &str,
    params: &RuleQueryParams,
) -> bool {
    if let Some(keyword) = params
        .keyword
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let lower_kw = keyword.to_lowercase();
        let in_name = name.to_lowercase().contains(&lower_kw);
        let in_desc = description.to_lowercase().contains(&lower_kw);
        if !in_name && !in_desc {
            return false;
        }
    }
    if let Some(want_type) = params
        .rule_type
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        && want_type != rule_type
    {
        return false;
    }
    true
}

fn room_status_label(status: &crate::domain::room::RoomStatus) -> &'static str {
    match status {
        crate::domain::room::RoomStatus::Waiting => "waiting",
        crate::domain::room::RoomStatus::Playing => "playing",
        crate::domain::room::RoomStatus::Finished => "finished",
    }
}

fn build_summary(
    id: String,
    name: String,
    description: String,
    created_at: i64,
    player_count: u8,
    developer: MarketDeveloper,
) -> PublishedRuleSummary {
    PublishedRuleSummary {
        id,
        name,
        description,
        rule_type: DEFAULT_RULE_TYPE.to_string(),
        developer,
        rating: 0.0,
        review_count: 0,
        published_at: created_at,
        cover_url: None,
        player_count,
    }
}

pub async fn list_published_rules(
    State(store): State<RuleStore>,
    State(user_repo): State<Arc<UserRepository>>,
    Query(params): Query<RuleQueryParams>,
) -> Result<Json<ApiResponse<Vec<PublishedRuleSummary>>>, AppError> {
    let snapshot: Vec<(String, String, String, String, i64, u8)> = {
        let guard = store.read().await;
        guard
            .published
            .values()
            .map(|rule| {
                (
                    rule.id.clone(),
                    rule.owner_id.clone(),
                    rule.name.clone(),
                    rule.description.clone(),
                    rule.created_at,
                    rule.player_count,
                )
            })
            .collect()
    };

    let mut summaries = Vec::with_capacity(snapshot.len());
    for (id, owner_id, name, description, created_at, player_count) in snapshot {
        if !matches_filter(&name, &description, DEFAULT_RULE_TYPE, &params) {
            continue;
        }
        let developer = resolve_developer(&user_repo, &owner_id).await;
        summaries.push(build_summary(
            id,
            name,
            description,
            created_at,
            player_count,
            developer,
        ));
    }

    summaries.sort_by_key(|s| std::cmp::Reverse(s.published_at));
    Ok(Json(ApiResponse::success(summaries)))
}

pub async fn get_published_rule_detail(
    State(store): State<RuleStore>,
    State(user_repo): State<Arc<UserRepository>>,
    Path(rule_id): Path<String>,
) -> Result<Json<ApiResponse<PublishedRuleDetail>>, AppError> {
    let snapshot = {
        let guard = store.read().await;
        guard.published.get(&rule_id).cloned()
    };
    let rule = snapshot.ok_or(AppError::NotFound)?;

    let developer = resolve_developer(&user_repo, &rule.owner_id).await;
    let introduction = placeholder_introduction(&rule.description);

    let summary = build_summary(
        rule.id,
        rule.name,
        rule.description,
        rule.created_at,
        rule.player_count,
        developer,
    );

    let detail = PublishedRuleDetail {
        summary,
        introduction,
        screenshots: Vec::new(),
        reviews: Vec::new(),
    };

    Ok(Json(ApiResponse::success(detail)))
}

pub async fn list_developer_rules(
    State(store): State<RuleStore>,
    State(user_repo): State<Arc<UserRepository>>,
    Path(developer_id): Path<String>,
    Query(params): Query<RuleQueryParams>,
) -> Result<Json<ApiResponse<Vec<PublishedRuleSummary>>>, AppError> {
    let developer = resolve_developer(&user_repo, &developer_id).await;
    let snapshot: Vec<(String, String, String, i64, u8)> = {
        let guard = store.read().await;
        guard
            .published
            .values()
            .filter(|rule| rule.owner_id == developer_id)
            .map(|rule| {
                (
                    rule.id.clone(),
                    rule.name.clone(),
                    rule.description.clone(),
                    rule.created_at,
                    rule.player_count,
                )
            })
            .collect()
    };

    let mut summaries = Vec::with_capacity(snapshot.len());
    for (id, name, description, created_at, player_count) in snapshot {
        if !matches_filter(&name, &description, DEFAULT_RULE_TYPE, &params) {
            continue;
        }
        summaries.push(build_summary(
            id,
            name,
            description,
            created_at,
            player_count,
            developer.clone(),
        ));
    }

    summaries.sort_by_key(|s| std::cmp::Reverse(s.published_at));
    Ok(Json(ApiResponse::success(summaries)))
}

pub async fn list_rooms_for_rule(
    State(room_store): State<RoomStore>,
    Path(rule_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<MarketRoomSummary>>>, AppError> {
    let guard = room_store.read().await;
    let mut rooms: Vec<MarketRoomSummary> = guard
        .rooms
        .values()
        .filter(|room| {
            room.rule_id == rule_id
                && !matches!(room.status, crate::domain::room::RoomStatus::Finished)
        })
        .map(|room| {
            let host_name = room
                .players
                .iter()
                .find(|p| p.id == room.host_id)
                .map(|p| p.username.clone())
                .unwrap_or_else(|| "未知房主".to_string());
            MarketRoomSummary {
                id: room.id.clone(),
                code: room.code.clone(),
                host_name,
                current_players: room.players.len(),
                max_players: room.player_count,
                has_password: room.password.is_some(),
                status: room_status_label(&room.status).to_string(),
            }
        })
        .collect();

    rooms.sort_by(|a, b| a.code.cmp(&b.code));
    Ok(Json(ApiResponse::success(rooms)))
}
