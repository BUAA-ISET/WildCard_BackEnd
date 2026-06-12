use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row, postgres::PgRow};
use uuid::Uuid;

use crate::{
    domain::report::{Report, ReportAction, ReportActionLog, ReportStatus, ReportTargetType},
    error::AppError,
    infrastructure::user::UserRepository,
    interface::auth::TokenClaims,
    interface::rule::{ApiResponse, ensure_admin},
};

/// 提交举报请求体。字段逐字对齐 FE `SubmitReportPayload`。
/// 注意：`reporter_id` 前端会传，但后端**不信任**它——以 TokenClaims.user_id 为准（防伪造）。
/// `reporter_name` / `reporter_avatar` 优先用 user_repo 查到的值，查不到再用前端兜底。
#[derive(Debug, Deserialize)]
pub struct SubmitReportPayload {
    #[serde(rename = "reporterId", default)]
    #[allow(dead_code)]
    pub reporter_id: String,
    #[serde(rename = "reporterName", default)]
    pub reporter_name: String,
    #[serde(rename = "reporterAvatar", default)]
    pub reporter_avatar: String,
    #[serde(rename = "targetType")]
    pub target_type: ReportTargetType,
    #[serde(rename = "targetId", default)]
    pub target_id: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub details: String,
    #[serde(default)]
    pub context: Option<serde_json::Value>,
}

/// 处理举报请求体，对齐 FE `ReportActionPayload`。`params` 前端可能传任意调参，
/// 后端目前只记状态不执行，因此接收但不使用（保留字段避免反序列化失败）。
#[derive(Debug, Deserialize)]
pub struct ReportActionPayload {
    pub action: ReportAction,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub params: Option<serde_json::Value>,
}

/// GET /api/reports 的查询参数，对齐 FE `ReportQuery`。
/// FE 在 status / targetType 为 "all" 时不会带上该参数，所以这里都是 Option。
#[derive(Debug, Default, Deserialize)]
pub struct ReportListQuery {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default, rename = "targetType")]
    pub target_type: Option<String>,
    #[serde(default)]
    pub keyword: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub page: Option<u32>,
}

/// GET /api/reports/counts 的返回体：`{ pending: <number> }`。
#[derive(Debug, Serialize)]
pub struct ReportCounts {
    pub pending: i64,
}

#[derive(Clone)]
pub struct ReportPersistence {
    pub pool: PgPool,
}

/// 判断一条举报是否命中关键字（大小写不敏感，模糊匹配 target_id / reason / details /
/// reporter_name / context.targetLabel）。抽成纯函数便于单测，口径与 FE filterLocalReports 对齐。
pub fn report_matches_keyword(report: &Report, keyword: &str) -> bool {
    let needle = keyword.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    let target_label = report
        .context
        .as_ref()
        .and_then(|ctx| ctx.get("targetLabel"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    [
        report.target_id.as_str(),
        report.reason.as_str(),
        report.details.as_str(),
        report.reporter_name.as_str(),
        target_label,
    ]
    .iter()
    .any(|field| field.to_lowercase().contains(&needle))
}

/// 把 context 规范化为"可选"：DB 里空对象 / JSON null 视为没有 context（返回 None），
/// 避免给 FE 推一个空 `{}` 触发空字段渲染逻辑。
fn normalize_context(value: serde_json::Value) -> Option<serde_json::Value> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::Object(ref map) if map.is_empty() => None,
        other => Some(other),
    }
}

impl ReportPersistence {
    pub async fn ensure_schema(&self) -> Result<(), AppError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS reports (
                id UUID PRIMARY KEY,
                reporter_id VARCHAR(128) NOT NULL,
                reporter_name VARCHAR(255) NOT NULL DEFAULT '',
                reporter_avatar VARCHAR(512) NOT NULL DEFAULT '',
                target_type VARCHAR(32) NOT NULL,
                target_id VARCHAR(255) NOT NULL,
                reason TEXT NOT NULL DEFAULT '',
                details TEXT NOT NULL DEFAULT '',
                status VARCHAR(32) NOT NULL DEFAULT 'pending',
                context JSONB NOT NULL DEFAULT '{}'::jsonb,
                action_log JSONB NOT NULL DEFAULT '[]'::jsonb,
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
            CREATE INDEX IF NOT EXISTS idx_reports_status_created
                ON reports(status, created_at DESC)
            "#,
        )
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        Ok(())
    }

    pub async fn insert(&self, report: &Report) -> Result<(), AppError> {
        let id = Uuid::parse_str(&report.id)
            .map_err(|e| AppError::InvalidInput(format!("举报 ID 必须是 UUID：{e}")))?;
        let context = report
            .context
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));
        let action_log = serde_json::to_value(&report.action_log).map_err(AppError::JsonError)?;

        sqlx::query(
            r#"
            INSERT INTO reports (
                id, reporter_id, reporter_name, reporter_avatar,
                target_type, target_id, reason, details, status,
                context, action_log,
                created_at, updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
                to_timestamp($12::double precision / 1000.0),
                to_timestamp($13::double precision / 1000.0)
            )
            "#,
        )
        .bind(id)
        .bind(&report.reporter_id)
        .bind(&report.reporter_name)
        .bind(&report.reporter_avatar)
        .bind(report.target_type.as_str())
        .bind(&report.target_id)
        .bind(&report.reason)
        .bind(&report.details)
        .bind(report.status.as_str())
        .bind(context)
        .bind(action_log)
        .bind(report.created_at)
        .bind(report.updated_at)
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        Ok(())
    }

    /// 按可选的 status / target_type 过滤，按 created_at DESC 排序拉取。
    /// keyword 过滤在上层用纯函数 `report_matches_keyword` 完成，便于单测。
    pub async fn list(
        &self,
        status: Option<ReportStatus>,
        target_type: Option<ReportTargetType>,
    ) -> Result<Vec<Report>, AppError> {
        let rows = sqlx::query(
            r#"
            SELECT
                id::text AS id,
                reporter_id::text AS reporter_id,
                reporter_name,
                reporter_avatar,
                target_type,
                target_id,
                reason,
                details,
                status,
                context,
                action_log,
                (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_at,
                (EXTRACT(EPOCH FROM updated_at) * 1000)::bigint AS updated_at
            FROM reports
            WHERE ($1::text IS NULL OR status = $1)
              AND ($2::text IS NULL OR target_type = $2)
            ORDER BY created_at DESC
            "#,
        )
        .bind(status.map(|s| s.as_str()))
        .bind(target_type.map(|t| t.as_str()))
        .fetch_all(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        rows.into_iter().map(row_to_report).collect()
    }

    pub async fn get(&self, id: &Uuid) -> Result<Option<Report>, AppError> {
        let row = sqlx::query(
            r#"
            SELECT
                id::text AS id,
                reporter_id::text AS reporter_id,
                reporter_name,
                reporter_avatar,
                target_type,
                target_id,
                reason,
                details,
                status,
                context,
                action_log,
                (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_at,
                (EXTRACT(EPOCH FROM updated_at) * 1000)::bigint AS updated_at
            FROM reports
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        row.map(row_to_report).transpose()
    }

    pub async fn count_pending(&self) -> Result<i64, AppError> {
        let row =
            sqlx::query("SELECT COUNT(*)::bigint AS count FROM reports WHERE status = 'pending'")
                .fetch_one(&self.pool)
                .await
                .inspect_err(|e| tracing::warn!("Database error {e}"))?;
        Ok(row.get::<i64, _>("count"))
    }

    /// 处理举报：更新 status / updated_at，并把整段 action_log 覆盖回写。
    pub async fn update_action(
        &self,
        id: &Uuid,
        status: ReportStatus,
        action_log: &[ReportActionLog],
        updated_at: i64,
    ) -> Result<(), AppError> {
        let action_log = serde_json::to_value(action_log).map_err(AppError::JsonError)?;
        sqlx::query(
            r#"
            UPDATE reports
            SET status = $2,
                action_log = $3,
                updated_at = to_timestamp($4::double precision / 1000.0)
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(status.as_str())
        .bind(action_log)
        .bind(updated_at)
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        Ok(())
    }
}

fn row_to_report(row: PgRow) -> Result<Report, AppError> {
    let context_raw: serde_json::Value = row.get("context");
    let action_log_raw: serde_json::Value = row.get("action_log");
    let action_log: Vec<ReportActionLog> =
        serde_json::from_value(action_log_raw).unwrap_or_default();

    Ok(Report {
        id: row.get("id"),
        reporter_id: row.get("reporter_id"),
        reporter_name: row.get("reporter_name"),
        reporter_avatar: row.get("reporter_avatar"),
        target_type: ReportTargetType::from_db_str(row.get::<String, _>("target_type").as_str())
            .unwrap_or(ReportTargetType::User),
        target_id: row.get("target_id"),
        reason: row.get("reason"),
        details: row.get("details"),
        status: ReportStatus::from_db_str(row.get::<String, _>("status").as_str()),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        context: normalize_context(context_raw),
        action_log,
    })
}

fn now_millis() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp_nanos() as i64 / 1_000_000
}

/// POST /api/reports —— 任意登录用户提交举报。
pub async fn submit_report(
    TokenClaims { user_id, .. }: TokenClaims,
    State(persistence): State<ReportPersistence>,
    State(user_repo): State<Arc<UserRepository>>,
    Json(payload): Json<SubmitReportPayload>,
) -> Result<Json<ApiResponse<Report>>, AppError> {
    if payload.reason.trim().is_empty() {
        return Err(AppError::InvalidInput("举报原因不能为空".to_string()));
    }
    if payload.target_id.trim().is_empty() {
        return Err(AppError::InvalidInput("举报对象不能为空".to_string()));
    }

    // 信任 token 里的 user_id 当 reporter_id，name/avatar 优先查 user_repo，查不到用前端兜底。
    let (reporter_name, reporter_avatar) = match user_repo.find_by_id(&user_id).await {
        Ok(Some(user)) => (user.name, user.avatar),
        _ => (
            payload.reporter_name.clone(),
            payload.reporter_avatar.clone(),
        ),
    };

    let now = now_millis();
    let report = Report {
        id: Uuid::new_v4().to_string(),
        reporter_id: user_id.to_string(),
        reporter_name,
        reporter_avatar,
        target_type: payload.target_type,
        target_id: payload.target_id,
        reason: payload.reason,
        details: payload.details,
        status: ReportStatus::Pending,
        created_at: now,
        updated_at: now,
        context: payload.context.and_then(normalize_context),
        action_log: Vec::new(),
    };

    persistence.insert(&report).await?;
    Ok(Json(ApiResponse::success(report)))
}

/// GET /api/reports —— 仅管理员，支持 status / targetType / keyword 过滤，created_at DESC。
pub async fn list_reports(
    TokenClaims { user_id, .. }: TokenClaims,
    State(persistence): State<ReportPersistence>,
    State(user_repo): State<Arc<UserRepository>>,
    Query(query): Query<ReportListQuery>,
) -> Result<Json<ApiResponse<Vec<Report>>>, AppError> {
    ensure_admin(&user_id, &user_repo).await?;

    // status / targetType 非法值直接当作"无过滤"忽略（FE 只会传合法值或省略）。
    let status = query
        .status
        .as_deref()
        .filter(|s| !s.is_empty() && *s != "all")
        .map(ReportStatus::from_db_str);
    let target_type = query
        .target_type
        .as_deref()
        .filter(|t| !t.is_empty() && *t != "all")
        .and_then(ReportTargetType::from_db_str);

    let mut reports = persistence.list(status, target_type).await?;

    if let Some(keyword) = query
        .keyword
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
    {
        reports.retain(|report| report_matches_keyword(report, keyword));
    }

    Ok(Json(ApiResponse::success(reports)))
}

/// GET /api/reports/counts —— 仅管理员，返回待处理数量。
/// 路由必须注册在 `/api/reports/{id}` 之前，否则 "counts" 会被当成 id 匹配。
pub async fn report_counts(
    TokenClaims { user_id, .. }: TokenClaims,
    State(persistence): State<ReportPersistence>,
    State(user_repo): State<Arc<UserRepository>>,
) -> Result<Json<ApiResponse<ReportCounts>>, AppError> {
    ensure_admin(&user_id, &user_repo).await?;
    let pending = persistence.count_pending().await?;
    Ok(Json(ApiResponse::success(ReportCounts { pending })))
}

/// GET /api/reports/{id} —— 仅管理员，不存在返 404。
pub async fn get_report(
    TokenClaims { user_id, .. }: TokenClaims,
    State(persistence): State<ReportPersistence>,
    State(user_repo): State<Arc<UserRepository>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Report>>, AppError> {
    ensure_admin(&user_id, &user_repo).await?;
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::NotFound)?;
    let report = persistence.get(&uuid).await?.ok_or(AppError::NotFound)?;
    Ok(Json(ApiResponse::success(report)))
}

/// POST /api/reports/{id}/action —— 仅管理员处理举报。
/// dismiss → rejected；ban_user / ban_rule → resolved。只记状态 + 追加 action_log，不真正执行。
pub async fn action_report(
    TokenClaims { user_id, .. }: TokenClaims,
    State(persistence): State<ReportPersistence>,
    State(user_repo): State<Arc<UserRepository>>,
    Path(id): Path<String>,
    Json(payload): Json<ReportActionPayload>,
) -> Result<Json<ApiResponse<Report>>, AppError> {
    ensure_admin(&user_id, &user_repo).await?;
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::NotFound)?;
    let mut report = persistence.get(&uuid).await?.ok_or(AppError::NotFound)?;

    let now = now_millis();
    let status = payload.action.resulting_status();
    report.action_log.push(ReportActionLog {
        id: format!("action-{now}"),
        action: payload.action,
        operator_id: user_id.to_string(),
        note: payload.note.unwrap_or_default(),
        created_at: now,
    });
    report.status = status;
    report.updated_at = now;

    persistence
        .update_action(&uuid, status, &report.action_log, now)
        .await?;

    Ok(Json(ApiResponse::success(report)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_report() -> Report {
        Report {
            id: Uuid::new_v4().to_string(),
            reporter_id: Uuid::new_v4().to_string(),
            reporter_name: "举报人甲".to_string(),
            reporter_avatar: "/static/a.png".to_string(),
            target_type: ReportTargetType::PlayerBehavior,
            target_id: "room-ABC".to_string(),
            reason: "言语辱骂".to_string(),
            details: "在对局中持续辱骂其他玩家".to_string(),
            status: ReportStatus::Pending,
            created_at: 1_700_000_000_000,
            updated_at: 1_700_000_000_000,
            context: Some(serde_json::json!({"targetLabel": "房间 ABC", "roomCode": "ABC"})),
            action_log: vec![],
        }
    }

    #[test]
    fn report_serializes_camel_case_and_roundtrips() {
        let report = sample_report();
        let json = serde_json::to_value(&report).unwrap();
        assert!(json.get("reporterId").and_then(|v| v.as_str()).is_some());
        assert_eq!(
            json.get("targetType").and_then(|v| v.as_str()),
            Some("player_behavior")
        );
        assert_eq!(json.get("status").and_then(|v| v.as_str()), Some("pending"));
        assert!(json.get("createdAt").and_then(|v| v.as_i64()).is_some());
        assert!(json.get("actionLog").and_then(|v| v.as_array()).is_some());
        // 往返：序列化再反序列化字段保持一致。
        let back: Report = serde_json::from_value(json).unwrap();
        assert_eq!(back.target_id, report.target_id);
        assert_eq!(back.target_type, ReportTargetType::PlayerBehavior);
        assert_eq!(back.status, ReportStatus::Pending);
    }

    #[test]
    fn report_skips_empty_avatar_and_none_context() {
        let mut report = sample_report();
        report.reporter_avatar = String::new();
        report.context = None;
        let json = serde_json::to_value(&report).unwrap();
        assert!(
            json.get("reporterAvatar").is_none(),
            "空头像不应出现在 JSON"
        );
        assert!(json.get("context").is_none(), "无 context 不应出现在 JSON");
    }

    #[test]
    fn submit_payload_deserializes_camel_case() {
        let payload = serde_json::json!({
            "reporterId": "fake-id",
            "reporterName": "前端传的名字",
            "reporterAvatar": "/static/x.png",
            "targetType": "rule",
            "targetId": "rule_123",
            "reason": "抄袭",
            "details": "与某规则高度雷同",
            "context": {"ruleId": "rule_123"}
        });
        let req: SubmitReportPayload = serde_json::from_value(payload).unwrap();
        assert_eq!(req.reporter_id, "fake-id");
        assert_eq!(req.reporter_name, "前端传的名字");
        assert_eq!(req.target_type, ReportTargetType::Rule);
        assert_eq!(req.target_id, "rule_123");
        assert!(req.context.is_some());
    }

    #[test]
    fn submit_payload_optional_fields_default() {
        // 老 FE 可能不带 reporterAvatar / context，必须按空 / None 兜底而不是失败。
        let payload = serde_json::json!({
            "reporterId": "id",
            "reporterName": "n",
            "targetType": "user",
            "targetId": "u-1",
            "reason": "r",
            "details": "d"
        });
        let req: SubmitReportPayload = serde_json::from_value(payload).unwrap();
        assert_eq!(req.reporter_avatar, "");
        assert!(req.context.is_none());
        assert_eq!(req.target_type, ReportTargetType::User);
    }

    #[test]
    fn submit_payload_rejects_unknown_target_type() {
        let payload = serde_json::json!({
            "targetType": "spaceship",
            "targetId": "x",
            "reason": "r",
            "details": "d"
        });
        let err = serde_json::from_value::<SubmitReportPayload>(payload);
        assert!(err.is_err(), "非法 targetType 应反序列化失败 → 上层 400");
    }

    #[test]
    fn action_payload_deserializes_camel_case() {
        let payload = serde_json::json!({"action": "ban_user", "note": "证据确凿"});
        let req: ReportActionPayload = serde_json::from_value(payload).unwrap();
        assert_eq!(req.action, ReportAction::BanUser);
        assert_eq!(req.note.as_deref(), Some("证据确凿"));
        assert!(req.params.is_none());
    }

    #[test]
    fn action_payload_rejects_unknown_action() {
        let payload = serde_json::json!({"action": "delete_universe"});
        assert!(serde_json::from_value::<ReportActionPayload>(payload).is_err());
    }

    #[test]
    fn action_to_status_mapping() {
        // 核心状态映射纯函数：dismiss → rejected，其余 → resolved。
        assert_eq!(
            ReportAction::Dismiss.resulting_status(),
            ReportStatus::Rejected
        );
        assert_eq!(
            ReportAction::BanUser.resulting_status(),
            ReportStatus::Resolved
        );
        assert_eq!(
            ReportAction::BanRule.resulting_status(),
            ReportStatus::Resolved
        );
    }

    #[test]
    fn status_from_db_str_falls_back_to_pending() {
        assert_eq!(
            ReportStatus::from_db_str("resolved"),
            ReportStatus::Resolved
        );
        assert_eq!(
            ReportStatus::from_db_str("rejected"),
            ReportStatus::Rejected
        );
        assert_eq!(ReportStatus::from_db_str(""), ReportStatus::Pending);
        assert_eq!(ReportStatus::from_db_str("garbage"), ReportStatus::Pending);
    }

    #[test]
    fn target_type_from_db_str_rejects_unknown() {
        assert_eq!(
            ReportTargetType::from_db_str("player_behavior"),
            Some(ReportTargetType::PlayerBehavior)
        );
        assert_eq!(ReportTargetType::from_db_str("nope"), None);
    }

    #[test]
    fn target_type_db_string_roundtrip() {
        for t in [
            ReportTargetType::User,
            ReportTargetType::Rule,
            ReportTargetType::Review,
            ReportTargetType::PlayerBehavior,
        ] {
            assert_eq!(ReportTargetType::from_db_str(t.as_str()), Some(t));
        }
    }

    #[test]
    fn action_log_append_records_operator_and_status() {
        // 模拟 action_report 的追加逻辑：push 一条 log 并切到目标状态。
        let mut report = sample_report();
        let now = 1_700_000_001_000;
        let status = ReportAction::Dismiss.resulting_status();
        report.action_log.push(ReportActionLog {
            id: format!("action-{now}"),
            action: ReportAction::Dismiss,
            operator_id: "admin-uuid".to_string(),
            note: "理由不充分".to_string(),
            created_at: now,
        });
        report.status = status;
        report.updated_at = now;

        assert_eq!(report.status, ReportStatus::Rejected);
        assert_eq!(report.action_log.len(), 1);
        assert_eq!(report.action_log[0].operator_id, "admin-uuid");
        assert_eq!(report.action_log[0].action, ReportAction::Dismiss);
        assert_eq!(report.updated_at, now);
        // 再处理一次应追加而非覆盖。
        report.action_log.push(ReportActionLog {
            id: "action-2".to_string(),
            action: ReportAction::BanUser,
            operator_id: "admin-uuid".to_string(),
            note: String::new(),
            created_at: now + 1,
        });
        assert_eq!(report.action_log.len(), 2);
    }

    #[test]
    fn keyword_filter_matches_across_fields_case_insensitive() {
        let report = sample_report();
        // target_id 命中（大小写不敏感）。
        assert!(report_matches_keyword(&report, "room-abc"));
        // reason 命中。
        assert!(report_matches_keyword(&report, "辱骂"));
        // reporter_name 命中。
        assert!(report_matches_keyword(&report, "举报人甲"));
        // context.targetLabel 命中。
        assert!(report_matches_keyword(&report, "房间 ABC"));
        // 不命中。
        assert!(!report_matches_keyword(&report, "不存在的关键字"));
        // 空 / 空白关键字视为全部命中。
        assert!(report_matches_keyword(&report, ""));
        assert!(report_matches_keyword(&report, "   "));
    }

    #[test]
    fn keyword_filter_handles_missing_context() {
        let mut report = sample_report();
        report.context = None;
        assert!(report_matches_keyword(&report, "言语"));
        assert!(!report_matches_keyword(&report, "房间"));
    }

    #[test]
    fn normalize_context_treats_empty_and_null_as_none() {
        assert!(normalize_context(serde_json::json!({})).is_none());
        assert!(normalize_context(serde_json::Value::Null).is_none());
        assert!(normalize_context(serde_json::json!({"roomCode": "X"})).is_some());
    }

    #[test]
    fn action_log_serializes_camel_case() {
        let log = ReportActionLog {
            id: "action-1".to_string(),
            action: ReportAction::BanRule,
            operator_id: "op-1".to_string(),
            note: "下架".to_string(),
            created_at: 123,
        };
        let json = serde_json::to_value(&log).unwrap();
        assert_eq!(
            json.get("operatorId").and_then(|v| v.as_str()),
            Some("op-1")
        );
        assert_eq!(json.get("createdAt").and_then(|v| v.as_i64()), Some(123));
        assert_eq!(
            json.get("action").and_then(|v| v.as_str()),
            Some("ban_rule")
        );
    }
}
