use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Json,
    extract::{Multipart, Path, Query, State},
};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::error::AppError;
use crate::infrastructure::user::UserRepository;
use crate::interface::auth::TokenClaims;
use crate::interface::rule::{ApiResponse, RulePersistence};
use crate::interface::user::extension_for_mime;
use crate::state::{RoomStore, RuleStore, UploadDir};

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
    State(persistence): State<RulePersistence>,
    Query(params): Query<RuleQueryParams>,
) -> Result<Json<ApiResponse<Vec<PublishedRuleSummary>>>, AppError> {
    let snapshot: Vec<(String, String, String, String, i64, u8, String)> = {
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
                    rule.cover_url.clone(),
                )
            })
            .collect()
    };

    let mut summaries = Vec::with_capacity(snapshot.len());
    for (id, owner_id, name, description, created_at, player_count, cover_url) in snapshot {
        if !matches_filter(&name, &description, DEFAULT_RULE_TYPE, &params) {
            continue;
        }
        let developer = resolve_developer(&user_repo, &owner_id).await;
        let mut summary = build_summary(id, name, description, created_at, player_count, developer);
        if !cover_url.is_empty() {
            summary.cover_url = Some(cover_url);
        }
        summaries.push(summary);
    }

    summaries.sort_by_key(|s| std::cmp::Reverse(s.published_at));
    let rule_ids: Vec<String> = summaries.iter().map(|s| s.id.clone()).collect();
    let stats_map = fetch_rating_stats_map(&persistence, &rule_ids).await;
    for summary in summaries.iter_mut() {
        if let Some(stats) = stats_map.get(&summary.id) {
            summary.rating = stats.average;
            summary.review_count = stats.count;
        }
    }
    Ok(Json(ApiResponse::success(summaries)))
}

pub async fn get_published_rule_detail(
    State(store): State<RuleStore>,
    State(user_repo): State<Arc<UserRepository>>,
    State(persistence): State<RulePersistence>,
    Path(rule_id): Path<String>,
) -> Result<Json<ApiResponse<PublishedRuleDetail>>, AppError> {
    let snapshot = {
        let guard = store.read().await;
        guard.published.get(&rule_id).cloned()
    };
    let rule = snapshot.ok_or(AppError::NotFound)?;

    let developer = resolve_developer(&user_repo, &rule.owner_id).await;
    // Phase 1B：introduction / cover / screenshots 现在存在 rule_published 行里，
    // 仅在作者没填时才回退到旧的 description-placeholder。
    let introduction = if rule.introduction.is_empty() {
        placeholder_introduction(&rule.description)
    } else {
        rule.introduction.clone()
    };
    let cover_url = if rule.cover_url.is_empty() {
        None
    } else {
        Some(rule.cover_url.clone())
    };
    let screenshots = rule.screenshot_urls.clone();

    let mut summary = build_summary(
        rule.id,
        rule.name,
        rule.description,
        rule.created_at,
        rule.player_count,
        developer,
    );
    summary.cover_url = cover_url;

    if let Ok(rule_uuid) = extract_rule_uuid(&summary.id) {
        if let Ok(stats) = fetch_rating_stats(&persistence, rule_uuid).await {
            summary.rating = stats.average;
            summary.review_count = stats.count;
        }
        let reviews = fetch_reviews(&persistence, rule_uuid, 20)
            .await
            .unwrap_or_default();
        let detail = PublishedRuleDetail {
            summary,
            introduction,
            screenshots,
            reviews,
        };
        return Ok(Json(ApiResponse::success(detail)));
    }

    let detail = PublishedRuleDetail {
        summary,
        introduction,
        screenshots,
        reviews: Vec::new(),
    };
    Ok(Json(ApiResponse::success(detail)))
}

pub async fn list_developer_rules(
    State(store): State<RuleStore>,
    State(user_repo): State<Arc<UserRepository>>,
    State(persistence): State<RulePersistence>,
    Path(developer_id): Path<String>,
    Query(params): Query<RuleQueryParams>,
) -> Result<Json<ApiResponse<Vec<PublishedRuleSummary>>>, AppError> {
    let developer = resolve_developer(&user_repo, &developer_id).await;
    let snapshot: Vec<(String, String, String, i64, u8, String)> = {
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
                    rule.cover_url.clone(),
                )
            })
            .collect()
    };

    let mut summaries = Vec::with_capacity(snapshot.len());
    for (id, name, description, created_at, player_count, cover_url) in snapshot {
        if !matches_filter(&name, &description, DEFAULT_RULE_TYPE, &params) {
            continue;
        }
        let mut summary = build_summary(
            id,
            name,
            description,
            created_at,
            player_count,
            developer.clone(),
        );
        if !cover_url.is_empty() {
            summary.cover_url = Some(cover_url);
        }
        summaries.push(summary);
    }

    summaries.sort_by_key(|s| std::cmp::Reverse(s.published_at));
    let rule_ids: Vec<String> = summaries.iter().map(|s| s.id.clone()).collect();
    let stats_map = fetch_rating_stats_map(&persistence, &rule_ids).await;
    for summary in summaries.iter_mut() {
        if let Some(stats) = stats_map.get(&summary.id) {
            summary.rating = stats.average;
            summary.review_count = stats.count;
        }
    }
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

// ---- Reviews ----

#[derive(Debug, Deserialize)]
pub struct CreateReviewRequest {
    pub rating: u8,
    #[serde(default)]
    pub content: String,
    #[serde(default, rename = "imageUrl")]
    pub image_url: Option<String>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct RuleRatingStats {
    pub average: f32,
    pub count: u32,
}

/// 把规则的字符串 ID 映射成 `rule_reviews.rule_id` (UUID 列) 用的稳定 UUID。
///
/// 三种 ID 形态：
/// 1. 标准 UUID（极少手写）→ 直接 parse
/// 2. 用户发布规则 `rule_<uuid>` → strip 前缀后 parse
/// 3. builtin 规则 `builtin-xxx-rule` / `party` / `classic` 等字符串
///    → 用 UUID v5 + WildCard 命名空间确定性派生一个稳定 UUID。
///    同名 builtin 永远映射到同一个 UUID，跨进程重启 / 多机部署都一致，
///    不污染 DB schema 也不需要在内存里维护映射表。
fn extract_rule_uuid(rule_id: &str) -> Result<uuid::Uuid, AppError> {
    let trimmed = rule_id.strip_prefix("rule_").unwrap_or(rule_id);
    if let Ok(parsed) = uuid::Uuid::parse_str(trimmed) {
        return Ok(parsed);
    }
    const BUILTIN_NAMESPACE: uuid::Uuid =
        uuid::Uuid::from_u128(0x7c9ab1c6_4d36_4ed2_9aa7_2c1f55f5f5f5);
    Ok(uuid::Uuid::new_v5(&BUILTIN_NAMESPACE, rule_id.as_bytes()))
}

async fn fetch_rating_stats(
    persistence: &RulePersistence,
    rule_uuid: uuid::Uuid,
) -> Result<RuleRatingStats, AppError> {
    let row = sqlx::query(
        r#"
        SELECT
            COALESCE(AVG(rating)::float4, 0.0) AS avg,
            COUNT(*)::int4 AS cnt
        FROM rule_reviews
        WHERE rule_id = $1
        "#,
    )
    .bind(rule_uuid)
    .fetch_one(&persistence.pool)
    .await
    .map_err(AppError::DatabaseError)?;
    Ok(RuleRatingStats {
        average: row.get::<f32, _>("avg"),
        count: row.get::<i32, _>("cnt") as u32,
    })
}

async fn fetch_rating_stats_map(
    persistence: &RulePersistence,
    rule_ids: &[String],
) -> HashMap<String, RuleRatingStats> {
    // 同一个 rule_id 可能映射到多份请求里（理论上不会但稳一点），用 vec 保 1:1。
    let pairs: Vec<(String, uuid::Uuid)> = rule_ids
        .iter()
        .filter_map(|id| extract_rule_uuid(id).ok().map(|u| (id.clone(), u)))
        .collect();
    let mut map = HashMap::new();
    if pairs.is_empty() {
        return map;
    }
    let uuids: Vec<uuid::Uuid> = pairs.iter().map(|(_, u)| *u).collect();
    let rows = match sqlx::query(
        r#"
        SELECT
            rule_id::text AS rule_id,
            COALESCE(AVG(rating)::float4, 0.0) AS avg,
            COUNT(*)::int4 AS cnt
        FROM rule_reviews
        WHERE rule_id = ANY($1)
        GROUP BY rule_id
        "#,
    )
    .bind(&uuids)
    .fetch_all(&persistence.pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("fetch_rating_stats_map failed: {e}");
            return map;
        }
    };
    // uuid → 原始 rule_id（可能多个 rule_id 映射到同一 uuid 的情况理论上有但不合理，
    // 这里 last-wins 足够）。
    let mut by_uuid: HashMap<uuid::Uuid, String> = HashMap::with_capacity(pairs.len());
    for (rule_id, uuid) in pairs {
        by_uuid.insert(uuid, rule_id);
    }
    for row in rows {
        let uuid_str: String = row.get("rule_id");
        let Ok(uuid) = uuid::Uuid::parse_str(&uuid_str) else {
            continue;
        };
        let Some(rule_id) = by_uuid.get(&uuid).cloned() else {
            continue;
        };
        let stats = RuleRatingStats {
            average: row.get::<f32, _>("avg"),
            count: row.get::<i32, _>("cnt") as u32,
        };
        map.insert(rule_id, stats);
    }
    map
}

async fn fetch_reviews(
    persistence: &RulePersistence,
    rule_uuid: uuid::Uuid,
    limit: i64,
) -> Result<Vec<RuleReview>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT
            r.id::text AS id,
            u.name AS author_name,
            u.avatar AS author_avatar,
            r.rating,
            r.content,
            r.image_url,
            (EXTRACT(EPOCH FROM r.created_at) * 1000)::bigint AS created_at
        FROM rule_reviews r
        JOIN users u ON u.id = r.author_id
        WHERE r.rule_id = $1
        ORDER BY r.created_at DESC
        LIMIT $2
        "#,
    )
    .bind(rule_uuid)
    .bind(limit)
    .fetch_all(&persistence.pool)
    .await
    .map_err(AppError::DatabaseError)?;

    Ok(rows
        .into_iter()
        .map(|row| RuleReview {
            id: row.get("id"),
            author_name: row.get("author_name"),
            author_avatar: row.get("author_avatar"),
            rating: row.get::<i16, _>("rating") as u8,
            content: row.get("content"),
            image_url: row.get::<Option<String>, _>("image_url"),
            created_at: row.get("created_at"),
        })
        .collect())
}

pub async fn create_review(
    TokenClaims { user_id, .. }: TokenClaims,
    State(user_repo): State<std::sync::Arc<UserRepository>>,
    State(persistence): State<RulePersistence>,
    State(store): State<RuleStore>,
    Path(rule_id): Path<String>,
    Json(payload): Json<CreateReviewRequest>,
) -> Result<Json<ApiResponse<RuleReview>>, AppError> {
    if payload.rating == 0 || payload.rating > 5 {
        return Err(AppError::InvalidInput("评分必须在 1 到 5 之间".to_string()));
    }

    // 校验规则存在
    {
        let guard = store.read().await;
        if !guard.published.contains_key(&rule_id) {
            return Err(AppError::NotFound);
        }
    }

    let rule_uuid = extract_rule_uuid(&rule_id)?;
    let review_id = uuid::Uuid::new_v4();
    let content = payload.content.trim().to_string();
    let image_url = payload
        .image_url
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    if let Some(url) = image_url.as_ref()
        && url.len() > 512
    {
        return Err(AppError::InvalidInput(
            "图片地址过长，请先调用图片上传接口拿到短 URL".to_string(),
        ));
    }

    sqlx::query(
        r#"
        INSERT INTO rule_reviews (id, rule_id, author_id, rating, content, image_url)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (rule_id, author_id) DO UPDATE SET
            rating = EXCLUDED.rating,
            content = EXCLUDED.content,
            image_url = EXCLUDED.image_url,
            updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind(review_id)
    .bind(rule_uuid)
    .bind(user_id.0)
    .bind(payload.rating as i16)
    .bind(&content)
    .bind(image_url.as_deref())
    .execute(&persistence.pool)
    .await
    .map_err(AppError::DatabaseError)?;

    // 查回评价者的展示名 (用户可能没填 avatar)
    let user = user_repo
        .find_by_id(&user_id)
        .await?
        .ok_or(AppError::Unauthorized("未登录".to_string()))?;

    let returned = RuleReview {
        id: review_id.to_string(),
        author_name: user.name,
        author_avatar: user.avatar,
        rating: payload.rating,
        content,
        image_url,
        created_at: now_millis(),
    };
    Ok(Json(ApiResponse::success(returned)))
}

fn now_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

const REVIEW_IMAGE_MAX_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadedImageResponse {
    pub image_url: String,
}

/// 接收 multipart 文件，落盘到 uploads/review-images/，返回短 URL。
/// 前端用这个 URL 再发给 POST /reviews 的 imageUrl 字段。
#[tracing::instrument(skip(multipart, upload_dir))]
pub async fn upload_review_image(
    TokenClaims { user_id: _, .. }: TokenClaims,
    State(upload_dir): State<UploadDir>,
    mut multipart: Multipart,
) -> Result<Json<ApiResponse<UploadedImageResponse>>, AppError> {
    let mut field = multipart
        .next_field()
        .await
        .map_err(|e| AppError::InvalidInput(format!("解析上传失败：{e}")))?
        .ok_or_else(|| AppError::InvalidInput("缺少上传文件".to_string()))?;

    let content_type = field.content_type().map(str::to_string).unwrap_or_default();
    let extension = extension_for_mime(&content_type)
        .ok_or_else(|| AppError::InvalidInput("仅支持 png / jpeg / webp 格式".to_string()))?;

    let mut bytes = Vec::with_capacity(16 * 1024);
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|e| AppError::InvalidInput(format!("读取上传内容失败：{e}")))?
    {
        if bytes.len() + chunk.len() > REVIEW_IMAGE_MAX_BYTES {
            return Err(AppError::InvalidInput("评论图片不能超过 2MB".to_string()));
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() {
        return Err(AppError::InvalidInput("上传文件为空".to_string()));
    }

    let dir = upload_dir.0.join("review-images");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| AppError::InvalidInput(format!("创建上传目录失败：{e}")))?;

    let filename = format!("{}.{extension}", uuid::Uuid::new_v4());
    let path = dir.join(&filename);
    tokio::fs::write(&path, &bytes)
        .await
        .map_err(|e| AppError::InvalidInput(format!("写入文件失败：{e}")))?;

    Ok(Json(ApiResponse::success(UploadedImageResponse {
        image_url: format!("/static/review-images/{filename}"),
    })))
}

#[cfg(test)]
mod tests {
    use super::{CreateReviewRequest, UploadedImageResponse, extract_rule_uuid};

    #[test]
    fn extract_rule_uuid_accepts_user_rule_format() {
        let id = "rule_550e8400-e29b-41d4-a716-446655440000";
        let uuid = extract_rule_uuid(id).expect("rule_<uuid> 应正常解析");
        assert_eq!(uuid.to_string(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn extract_rule_uuid_accepts_bare_uuid() {
        let id = "550e8400-e29b-41d4-a716-446655440000";
        let uuid = extract_rule_uuid(id).expect("裸 uuid 也应正常解析");
        assert_eq!(uuid.to_string(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn extract_rule_uuid_derives_stable_v5_for_builtin_ids() {
        // builtin id 没有 rule_ 前缀也不是 uuid，应走 v5 派生。
        let a1 = extract_rule_uuid("builtin-bigtwo-rule").unwrap();
        let a2 = extract_rule_uuid("builtin-bigtwo-rule").unwrap();
        assert_eq!(a1, a2, "同一 id 必须派生出相同 uuid（确定性）");
        assert_eq!(a1.get_version_num(), 5, "应是 v5");

        // 不同 id 必须派生出不同 uuid。
        let b = extract_rule_uuid("builtin-war-rule").unwrap();
        assert_ne!(a1, b);
        let c = extract_rule_uuid("party").unwrap();
        assert_ne!(a1, c);
        assert_ne!(b, c);
    }

    #[test]
    fn extract_rule_uuid_never_returns_err_after_v5_fallback() {
        // 任意杂字符串都应能映射出一个 uuid，而不是返回 InvalidInput。
        assert!(extract_rule_uuid("anything-goes").is_ok());
        assert!(extract_rule_uuid("").is_ok());
        assert!(extract_rule_uuid("中文 ID 也行").is_ok());
    }

    #[test]
    fn create_review_request_deserializes_camel_case_image_url() {
        let req: CreateReviewRequest = serde_json::from_str(
            r#"{"rating":5,"content":"带图评论","imageUrl":"/static/review-images/r.png"}"#,
        )
        .unwrap();

        assert_eq!(req.rating, 5);
        assert_eq!(req.content, "带图评论");
        assert_eq!(
            req.image_url.as_deref(),
            Some("/static/review-images/r.png")
        );
    }

    #[test]
    fn create_review_request_defaults_optional_content_and_image_url() {
        let req: CreateReviewRequest = serde_json::from_str(r#"{"rating":4}"#).unwrap();

        assert_eq!(req.rating, 4);
        assert_eq!(req.content, "");
        assert!(req.image_url.is_none());
    }

    #[test]
    fn uploaded_image_response_serializes_short_url_as_camel_case() {
        let response = UploadedImageResponse {
            image_url: "/static/review-images/r.png".to_string(),
        };
        let json = serde_json::to_value(response).unwrap();

        assert_eq!(
            json.get("imageUrl").and_then(|v| v.as_str()),
            Some("/static/review-images/r.png")
        );
        assert!(json.get("image_url").is_none());
    }
}
