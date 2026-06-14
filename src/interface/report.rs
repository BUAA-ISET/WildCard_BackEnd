use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row, postgres::PgRow};
use uuid::Uuid;

use crate::{
    domain::report::{
        Punishment, PunishmentScope, Report, ReportAction, ReportActionLog, ReportStatus,
        ReportTargetRule, ReportTargetType, ReportTargetUser, ReportUser, action_scope,
        same_punished_target, validate_action,
    },
    domain::user::UserId,
    error::AppError,
    infrastructure::user::UserRepository,
    interface::auth::TokenClaims,
    interface::rule::{
        ApiResponse, RulePersistence, can_ban_user, ensure_admin, ensure_not_banned,
    },
    state::RuleStore,
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

/// 处理动作的可选调参，对齐 FE `ReportActionParams`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReportActionParams {
    #[serde(
        default,
        rename = "targetType",
        skip_serializing_if = "Option::is_none"
    )]
    #[allow(dead_code)]
    pub target_type: Option<String>,
    #[serde(default, rename = "targetId", skip_serializing_if = "Option::is_none")]
    #[allow(dead_code)]
    pub target_id: Option<String>,
    #[serde(
        default,
        rename = "targetUserId",
        skip_serializing_if = "Option::is_none"
    )]
    pub target_user_id: Option<String>,
    #[serde(
        default,
        rename = "targetRuleId",
        skip_serializing_if = "Option::is_none"
    )]
    pub target_rule_id: Option<String>,
    #[serde(
        default,
        rename = "ruleAuthorId",
        skip_serializing_if = "Option::is_none"
    )]
    pub rule_author_id: Option<String>,
    #[serde(default, rename = "banDays", skip_serializing_if = "Option::is_none")]
    pub ban_days: Option<i64>,
}

/// 处理举报请求体，对齐 FE `ReportActionPayload`。`params` 携带封禁时长 / 目标覆盖等调参。
#[derive(Debug, Deserialize)]
pub struct ReportActionPayload {
    pub action: ReportAction,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub params: Option<ReportActionParams>,
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
    #[serde(default, rename = "targetUser")]
    pub target_user: Option<String>,
    #[serde(default, rename = "targetRule")]
    pub target_rule: Option<String>,
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

/// 派生被举报用户 ID，对齐 FE `getTargetUserId`：
/// 优先 targetUser.id，否则 user / player_behavior 类用 target_id，其余 None。
pub fn report_target_user_id(report: &Report) -> Option<String> {
    if let Some(tu) = &report.target_user
        && !tu.id.is_empty()
    {
        return Some(tu.id.clone());
    }
    match report.target_type {
        ReportTargetType::User | ReportTargetType::PlayerBehavior => Some(report.target_id.clone()),
        _ => None,
    }
}

/// 派生被举报规则 ID，对齐 FE `getTargetRuleId`：
/// 优先 targetRule.id，否则 context.ruleId，否则 rule / review 类用 target_id，其余 None。
pub fn report_target_rule_id(report: &Report) -> Option<String> {
    if let Some(tr) = &report.target_rule
        && !tr.id.is_empty()
    {
        return Some(tr.id.clone());
    }
    if let Some(rule_id) = report
        .context
        .as_ref()
        .and_then(|ctx| ctx.get("ruleId"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        return Some(rule_id.to_string());
    }
    match report.target_type {
        ReportTargetType::Rule | ReportTargetType::Review => Some(report.target_id.clone()),
        _ => None,
    }
}

/// 判断一条举报是否命中关键字（大小写不敏感）。匹配 target_id / reason / details /
/// reporter_name / context.targetLabel，以及结构化 target_user / target_rule 的 id+name。
/// 口径与 FE filterLocalReports 对齐。抽成纯函数便于单测。
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
    let mut haystack: Vec<String> = vec![
        report.target_id.clone(),
        report.reason.clone(),
        report.details.clone(),
        report.reporter_name.clone(),
        target_label.to_string(),
    ];
    if let Some(tu) = &report.target_user {
        haystack.push(tu.id.clone());
        if let Some(name) = &tu.name {
            haystack.push(name.clone());
        }
    }
    if let Some(tr) = &report.target_rule {
        haystack.push(tr.id.clone());
        if let Some(name) = &tr.name {
            haystack.push(name.clone());
        }
    }
    haystack
        .iter()
        .any(|field| field.to_lowercase().contains(&needle))
}

/// targetUser 过滤纯函数，对齐 FE：匹配 target_user.id / target_user.name / 派生 user id。
pub fn report_matches_target_user(report: &Report, query: &str) -> bool {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    let mut values: Vec<String> = Vec::new();
    if let Some(tu) = &report.target_user {
        values.push(tu.id.clone());
        if let Some(name) = &tu.name {
            values.push(name.clone());
        }
    }
    if let Some(id) = report_target_user_id(report) {
        values.push(id);
    }
    values.iter().any(|v| v.to_lowercase().contains(&needle))
}

/// targetRule 过滤纯函数，对齐 FE：匹配 target_rule.id / target_rule.name / 派生 rule id。
pub fn report_matches_target_rule(report: &Report, query: &str) -> bool {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    let mut values: Vec<String> = Vec::new();
    if let Some(tr) = &report.target_rule {
        values.push(tr.id.clone());
        if let Some(name) = &tr.name {
            values.push(name.clone());
        }
    }
    if let Some(id) = report_target_rule_id(report) {
        values.push(id);
    }
    values.iter().any(|v| v.to_lowercase().contains(&needle))
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

        // 合并联动：reports 加 merged_by_punishment_id，记录被哪条惩罚合并。幂等升级。
        sqlx::query(
            r#"
            ALTER TABLE reports
                ADD COLUMN IF NOT EXISTS merged_by_punishment_id VARCHAR(128)
            "#,
        )
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        // 惩罚独立表：一条 action 落地一条惩罚记录，撤销时逆转用。
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS punishments (
                id UUID PRIMARY KEY,
                report_id UUID NOT NULL,
                action VARCHAR(16) NOT NULL,
                scope VARCHAR(8) NOT NULL,
                active BOOLEAN NOT NULL DEFAULT true,
                ban_days INT,
                banned_until BIGINT,
                rule_removed BOOLEAN NOT NULL DEFAULT false,
                target_user_id VARCHAR(128),
                target_rule_id VARCHAR(128),
                affected_report_ids JSONB NOT NULL DEFAULT '[]'::jsonb,
                created_at BIGINT NOT NULL,
                revoked_at BIGINT
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_punishments_report ON punishments(report_id)
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
                context, action_log, merged_by_punishment_id,
                created_at, updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                to_timestamp($13::double precision / 1000.0),
                to_timestamp($14::double precision / 1000.0)
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
        .bind(&report.merged_by_punishment_id)
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
                merged_by_punishment_id,
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
                merged_by_punishment_id,
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

    /// 处理举报：更新 status / updated_at / merged_by_punishment_id，并把整段 action_log 覆盖回写。
    pub async fn update_action(
        &self,
        id: &Uuid,
        status: ReportStatus,
        action_log: &[ReportActionLog],
        merged_by_punishment_id: Option<&str>,
        updated_at: i64,
    ) -> Result<(), AppError> {
        let action_log = serde_json::to_value(action_log).map_err(AppError::JsonError)?;
        sqlx::query(
            r#"
            UPDATE reports
            SET status = $2,
                action_log = $3,
                merged_by_punishment_id = $4,
                updated_at = to_timestamp($5::double precision / 1000.0)
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(status.as_str())
        .bind(action_log)
        .bind(merged_by_punishment_id)
        .bind(updated_at)
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        Ok(())
    }

    /// 写入一条惩罚记录。affected_report_ids 以 JSONB 数组存储。
    pub async fn insert_punishment(&self, punishment: &Punishment) -> Result<(), AppError> {
        let id = Uuid::parse_str(&punishment.id)
            .map_err(|e| AppError::InvalidInput(format!("惩罚 ID 必须是 UUID：{e}")))?;
        let report_id = Uuid::parse_str(&punishment.report_id)
            .map_err(|e| AppError::InvalidInput(format!("举报 ID 必须是 UUID：{e}")))?;
        let affected =
            serde_json::to_value(&punishment.affected_report_ids).map_err(AppError::JsonError)?;
        sqlx::query(
            r#"
            INSERT INTO punishments (
                id, report_id, action, scope, active, ban_days, banned_until,
                rule_removed, target_user_id, target_rule_id, affected_report_ids,
                created_at, revoked_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(id)
        .bind(report_id)
        .bind(punishment.action.as_str())
        .bind(punishment.scope.as_str())
        .bind(punishment.active)
        .bind(punishment.ban_days.map(|d| d as i32))
        .bind(punishment.banned_until)
        .bind(punishment.rule_removed)
        .bind(&punishment.target_user_id)
        .bind(&punishment.target_rule_id)
        .bind(affected)
        .bind(punishment.created_at)
        .bind(punishment.revoked_at)
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        Ok(())
    }

    /// 查某条举报当前生效（active）的惩罚（撤销 / 读取补齐用）。
    pub async fn get_active_punishment_by_report(
        &self,
        report_id: &Uuid,
    ) -> Result<Option<Punishment>, AppError> {
        let row = sqlx::query(
            r#"
            SELECT
                id::text AS id,
                report_id::text AS report_id,
                action, scope, active, ban_days, banned_until, rule_removed,
                target_user_id, target_rule_id, affected_report_ids,
                created_at, revoked_at
            FROM punishments
            WHERE report_id = $1 AND active = true
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(report_id)
        .fetch_optional(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        row.map(row_to_punishment).transpose()
    }

    /// 撤销一条惩罚：active=false + revoked_at。
    pub async fn revoke_punishment(
        &self,
        punishment_id: &str,
        revoked_at: i64,
    ) -> Result<(), AppError> {
        let id = Uuid::parse_str(punishment_id)
            .map_err(|e| AppError::InvalidInput(format!("惩罚 ID 必须是 UUID：{e}")))?;
        sqlx::query(
            r#"
            UPDATE punishments
            SET active = false, revoked_at = $2
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(revoked_at)
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        Ok(())
    }

    /// 把一批举报标记为被某条惩罚合并：status=resolved + merged_by_punishment_id。
    pub async fn mark_reports_merged(
        &self,
        punishment_id: &str,
        report_ids: &[String],
        updated_at: i64,
    ) -> Result<(), AppError> {
        for rid in report_ids {
            let uuid = match Uuid::parse_str(rid) {
                Ok(u) => u,
                Err(_) => continue,
            };
            sqlx::query(
                r#"
                UPDATE reports
                SET status = 'resolved',
                    merged_by_punishment_id = $2,
                    updated_at = to_timestamp($3::double precision / 1000.0)
                WHERE id = $1
                "#,
            )
            .bind(uuid)
            .bind(punishment_id)
            .bind(updated_at)
            .execute(&self.pool)
            .await
            .inspect_err(|e| tracing::warn!("Database error {e}"))?;
        }
        Ok(())
    }

    /// 撤销惩罚时把被合并的举报退回 pending + 清 merged_by_punishment_id。
    pub async fn restore_merged_reports(
        &self,
        punishment_id: &str,
        updated_at: i64,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE reports
            SET status = 'pending',
                merged_by_punishment_id = NULL,
                updated_at = to_timestamp($2::double precision / 1000.0)
            WHERE merged_by_punishment_id = $1
            "#,
        )
        .bind(punishment_id)
        .bind(updated_at)
        .execute(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        Ok(())
    }

    /// 拉取若干 pending 举报用于合并判定（排除源举报，按 created_at DESC）。
    pub async fn list_pending_for_merge(&self, exclude_id: &Uuid) -> Result<Vec<Report>, AppError> {
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
                merged_by_punishment_id,
                (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_at,
                (EXTRACT(EPOCH FROM updated_at) * 1000)::bigint AS updated_at
            FROM reports
            WHERE status = 'pending' AND id <> $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(exclude_id)
        .fetch_all(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        rows.into_iter().map(row_to_report).collect()
    }
}

fn row_to_punishment(row: PgRow) -> Result<Punishment, AppError> {
    let action = ReportAction::from_db_str(row.get::<String, _>("action").as_str())
        .unwrap_or(ReportAction::BanUser);
    let scope = PunishmentScope::from_db_str(row.get::<String, _>("scope").as_str())
        .unwrap_or(PunishmentScope::User);
    let affected_raw: serde_json::Value = row.get("affected_report_ids");
    let affected_report_ids: Vec<String> = serde_json::from_value(affected_raw).unwrap_or_default();
    Ok(Punishment {
        id: row.get("id"),
        action,
        scope,
        active: row.get("active"),
        ban_days: row.get::<Option<i32>, _>("ban_days").map(|d| d as i64),
        banned_until: row.get("banned_until"),
        rule_removed: row.get("rule_removed"),
        affected_report_ids,
        created_at: row.get("created_at"),
        revoked_at: row.get("revoked_at"),
        report_id: row.get("report_id"),
        target_user_id: row.get("target_user_id"),
        target_rule_id: row.get("target_rule_id"),
    })
}

fn row_to_report(row: PgRow) -> Result<Report, AppError> {
    let context_raw: serde_json::Value = row.get("context");
    let action_log_raw: serde_json::Value = row.get("action_log");
    let action_log: Vec<ReportActionLog> =
        serde_json::from_value(action_log_raw).unwrap_or_default();
    let reporter_id: String = row.get("reporter_id");
    let reporter_name: String = row.get("reporter_name");
    let reporter_avatar: String = row.get("reporter_avatar");
    let merged_by_punishment_id: Option<String> = row.get("merged_by_punishment_id");

    // 同时填充结构化 reporter（新契约）与扁平字段（deprecated 兼容），FE 两边都读。
    let reporter = Some(ReportUser {
        id: reporter_id.clone(),
        name: reporter_name.clone(),
        avatar: reporter_avatar.clone(),
    });

    Ok(Report {
        id: row.get("id"),
        reporter,
        reporter_id,
        reporter_name,
        reporter_avatar,
        target_type: ReportTargetType::from_db_str(row.get::<String, _>("target_type").as_str())
            .unwrap_or(ReportTargetType::User),
        target_id: row.get("target_id"),
        target_user: None,
        target_rule: None,
        reason: row.get("reason"),
        details: row.get("details"),
        status: ReportStatus::from_db_str(row.get::<String, _>("status").as_str()),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        context: normalize_context(context_raw),
        punishment: None,
        merged_by_punishment_id,
        action_log,
    })
}

fn now_millis() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp_nanos() as i64 / 1_000_000
}

/// 读取时补齐结构化字段：target_user / target_rule（rule 类从 RuleStore 取 authorId/authorName）
/// + 当前 active punishment。纯读，不改 DB。
async fn enrich_report(
    report: &mut Report,
    persistence: &ReportPersistence,
    store: &RuleStore,
) -> Result<(), AppError> {
    // target_user：user / player_behavior 类按派生 id 填充。
    if report.target_user.is_none()
        && let Some(uid) = report_target_user_id(report)
    {
        let label = report
            .context
            .as_ref()
            .and_then(|ctx| ctx.get("targetLabel"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        report.target_user = Some(ReportTargetUser {
            id: uid,
            name: label,
            avatar: None,
        });
    }
    // target_rule：rule / review 类，从 RuleStore 补 name / authorId / authorName。
    if report.target_rule.is_none()
        && let Some(rid) = report_target_rule_id(report)
    {
        let (rule_name, author_id) = {
            let guard = store.read().await;
            match guard.published.get(&rid) {
                Some(rule) => (Some(rule.name.clone()), Some(rule.owner_id.clone())),
                None => (None, None),
            }
        };
        let label = report
            .context
            .as_ref()
            .and_then(|ctx| ctx.get("targetLabel"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        report.target_rule = Some(ReportTargetRule {
            id: rid,
            name: rule_name.or(label),
            author_id,
            author_name: None,
        });
    }
    // 当前生效惩罚。
    if let Ok(uuid) = Uuid::parse_str(&report.id) {
        report.punishment = persistence.get_active_punishment_by_report(&uuid).await?;
    }
    Ok(())
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

    ensure_not_banned(&user_id, &user_repo).await?;

    // 信任 token 里的 user_id 当 reporter_id，name/avatar 优先查 user_repo，查不到用前端兜底。
    let (reporter_name, reporter_avatar) = match user_repo.find_by_id(&user_id).await {
        Ok(Some(user)) => (user.name, user.avatar),
        _ => (
            payload.reporter_name.clone(),
            payload.reporter_avatar.clone(),
        ),
    };

    let now = now_millis();
    let reporter = Some(ReportUser {
        id: user_id.to_string(),
        name: reporter_name.clone(),
        avatar: reporter_avatar.clone(),
    });
    let report = Report {
        id: Uuid::new_v4().to_string(),
        reporter,
        reporter_id: user_id.to_string(),
        reporter_name,
        reporter_avatar,
        target_type: payload.target_type,
        target_id: payload.target_id,
        target_user: None,
        target_rule: None,
        reason: payload.reason,
        details: payload.details,
        status: ReportStatus::Pending,
        created_at: now,
        updated_at: now,
        context: payload.context.and_then(normalize_context),
        punishment: None,
        merged_by_punishment_id: None,
        action_log: Vec::new(),
    };

    persistence.insert(&report).await?;
    Ok(Json(ApiResponse::success(report)))
}

/// GET /api/reports —— 仅管理员，支持 status / targetType / keyword / targetUser / targetRule 过滤。
pub async fn list_reports(
    TokenClaims { user_id, .. }: TokenClaims,
    State(persistence): State<ReportPersistence>,
    State(user_repo): State<Arc<UserRepository>>,
    State(store): State<RuleStore>,
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

    // 补齐结构化字段（含 target_user / target_rule），过滤前先做，保证过滤口径与 FE 一致。
    for report in reports.iter_mut() {
        enrich_report(report, &persistence, &store).await?;
    }

    if let Some(keyword) = query
        .keyword
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
    {
        reports.retain(|report| report_matches_keyword(report, keyword));
    }
    if let Some(tu) = query
        .target_user
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
    {
        reports.retain(|report| report_matches_target_user(report, tu));
    }
    if let Some(tr) = query
        .target_rule
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
    {
        reports.retain(|report| report_matches_target_rule(report, tr));
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
    State(store): State<RuleStore>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Report>>, AppError> {
    ensure_admin(&user_id, &user_repo).await?;
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::NotFound)?;
    let mut report = persistence.get(&uuid).await?.ok_or(AppError::NotFound)?;
    enrich_report(&mut report, &persistence, &store).await?;
    Ok(Json(ApiResponse::success(report)))
}

/// POST /api/reports/{id}/action —— 仅管理员处理举报，完整惩罚模型：
/// - dismiss → rejected，无联动；
/// - ban_user / ban_rule / ban_both → resolved，按 scope 封用户（写 banned_until）/ 下架规则，
///   建 punishment 记录，并自动合并同目标的其它 pending 举报；
/// - revoke → 逆转该举报当前 active 惩罚（解封 / 恢复规则），退回被合并举报，源举报回 pending。
///
/// 校验口径逐一对齐 FE validateLocalAction。联动失败（如目标是 admin）在改库前返回，避免状态不一致。
pub async fn action_report(
    TokenClaims { user_id, .. }: TokenClaims,
    State(persistence): State<ReportPersistence>,
    State(user_repo): State<Arc<UserRepository>>,
    State(store): State<RuleStore>,
    State(rule_persistence): State<RulePersistence>,
    Path(id): Path<String>,
    Json(payload): Json<ReportActionPayload>,
) -> Result<Json<ApiResponse<Report>>, AppError> {
    ensure_admin(&user_id, &user_repo).await?;
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::NotFound)?;
    let mut report = persistence.get(&uuid).await?.ok_or(AppError::NotFound)?;
    // 补齐 target_rule.author_id / active punishment，校验和联动都依赖它。
    enrich_report(&mut report, &persistence, &store).await?;

    let params = payload.params.clone().unwrap_or_default();
    let ban_days = params.ban_days;
    // has_rule_author：targetRule.authorId 或 params.ruleAuthorId 任一存在。
    let rule_author_from_store = report
        .target_rule
        .as_ref()
        .and_then(|tr| tr.author_id.clone());
    let has_rule_author = rule_author_from_store.is_some() || params.rule_author_id.is_some();

    // 校验（对齐 FE validateLocalAction）。
    validate_action(
        payload.action,
        report.status,
        report.punishment.as_ref().is_some_and(|p| p.active),
        ban_days,
        report.target_type,
        has_rule_author,
    )
    .map_err(AppError::InvalidInput)?;

    let now = now_millis();

    match payload.action {
        ReportAction::Dismiss => {
            apply_status_and_log(&persistence, &uuid, &mut report, &payload, user_id.0, now)
                .await?;
        }
        ReportAction::Revoke => {
            // 取 active punishment（校验已保证存在）。
            let punishment = report.punishment.clone().ok_or_else(|| {
                AppError::InvalidInput("当前举报没有可撤销的有效惩罚".to_string())
            })?;
            // 逆转封禁 / 下架（按 scope）。
            if matches!(
                punishment.scope,
                PunishmentScope::User | PunishmentScope::Both
            ) && let Some(uid) = &punishment.target_user_id
                && let Ok(u) = Uuid::parse_str(uid)
            {
                user_repo.set_user_banned_until(&UserId(u), None).await?;
            }
            if matches!(
                punishment.scope,
                PunishmentScope::Rule | PunishmentScope::Both
            ) && let Some(rid) = &punishment.target_rule_id
            {
                rule_persistence.set_rule_banned(rid, false).await?;
                if let Some(rule) = store.write().await.published.get_mut(rid) {
                    rule.banned = false;
                }
            }
            // 惩罚置失效。
            persistence.revoke_punishment(&punishment.id, now).await?;
            // 被合并的举报退回 pending + 清 merged_by_punishment_id。
            persistence
                .restore_merged_reports(&punishment.id, now)
                .await?;
            // 源举报退回 pending（revoke → pending）。
            report.punishment = None;
            apply_status_and_log(&persistence, &uuid, &mut report, &payload, user_id.0, now)
                .await?;
        }
        ReportAction::BanUser | ReportAction::BanRule | ReportAction::BanBoth => {
            let scope = action_scope(payload.action)
                .ok_or_else(|| AppError::InvalidInput("不支持的处罚动作".to_string()))?;

            // 解析被封 user：FE 传 targetUserId / ruleAuthorId 优先，否则 user 类用 target_id，
            // rule/review 类用 RuleStore 查到的 owner_id。
            let target_user_id: Option<String> = match scope {
                PunishmentScope::Rule => None,
                _ => params
                    .target_user_id
                    .clone()
                    .or_else(|| params.rule_author_id.clone())
                    .or_else(|| match report.target_type {
                        ReportTargetType::User | ReportTargetType::PlayerBehavior => {
                            Some(report.target_id.clone())
                        }
                        _ => rule_author_from_store.clone(),
                    }),
            };

            // 解析被下架 rule：params.targetRuleId 优先，否则派生 rule id（含 context.ruleId / target_id）。
            let target_rule_id: Option<String> = match scope {
                PunishmentScope::User => None,
                _ => params
                    .target_rule_id
                    .clone()
                    .or_else(|| report_target_rule_id(&report)),
            };

            let mut banned_until: Option<i64> = None;
            // 执行封用户（写 banned_until）。
            if matches!(scope, PunishmentScope::User | PunishmentScope::Both) {
                let uid = target_user_id
                    .clone()
                    .ok_or_else(|| AppError::InvalidInput("缺少被封用户 ID".to_string()))?;
                let target_uuid = Uuid::parse_str(uid.trim()).map_err(|_| {
                    AppError::InvalidInput("封禁用户失败：被封对象 ID 不是合法用户 ID".to_string())
                })?;
                let target = user_repo
                    .find_by_id(&UserId(target_uuid))
                    .await?
                    .ok_or(AppError::NotFound)?;
                can_ban_user(&target.role)?;
                let days = ban_days.unwrap_or(0);
                let until = now + days * 86_400_000;
                user_repo
                    .set_user_banned_until(&UserId(target_uuid), Some(until))
                    .await?;
                banned_until = Some(until);
            }
            // 执行下架规则。
            let rule_removed = matches!(scope, PunishmentScope::Rule | PunishmentScope::Both);
            if rule_removed {
                let rid = target_rule_id
                    .clone()
                    .ok_or_else(|| AppError::InvalidInput("缺少被下架规则 ID".to_string()))?;
                rule_persistence.set_rule_banned(&rid, true).await?;
                if let Some(rule) = store.write().await.published.get_mut(&rid) {
                    rule.banned = true;
                }
            }

            // 建惩罚记录。
            let punishment_id = Uuid::new_v4().to_string();
            // 合并：扫描同目标 pending 举报。
            let candidates = persistence.list_pending_for_merge(&uuid).await?;
            let mut affected_report_ids: Vec<String> = Vec::new();
            for cand in &candidates {
                let cand_user = report_target_user_id(cand);
                let cand_rule = report_target_rule_id(cand);
                if same_punished_target(
                    scope,
                    target_user_id.as_deref(),
                    target_rule_id.as_deref(),
                    cand_user.as_deref(),
                    cand_rule.as_deref(),
                ) {
                    affected_report_ids.push(cand.id.clone());
                }
            }

            let punishment = Punishment {
                id: punishment_id.clone(),
                action: payload.action,
                scope,
                active: true,
                ban_days,
                banned_until,
                rule_removed,
                affected_report_ids: affected_report_ids.clone(),
                created_at: now,
                revoked_at: None,
                report_id: report.id.clone(),
                target_user_id: target_user_id.clone(),
                target_rule_id: target_rule_id.clone(),
            };
            persistence.insert_punishment(&punishment).await?;
            if !affected_report_ids.is_empty() {
                persistence
                    .mark_reports_merged(&punishment_id, &affected_report_ids, now)
                    .await?;
            }
            report.punishment = Some(punishment);
            apply_status_and_log(&persistence, &uuid, &mut report, &payload, user_id.0, now)
                .await?;
        }
    }

    Ok(Json(ApiResponse::success(report)))
}

/// 追加 action_log + 落地 status / updated_at（含 merged_by_punishment_id），并回写 DB。
async fn apply_status_and_log(
    persistence: &ReportPersistence,
    uuid: &Uuid,
    report: &mut Report,
    payload: &ReportActionPayload,
    operator_id: Uuid,
    now: i64,
) -> Result<(), AppError> {
    let status = payload.action.resulting_status();
    report.action_log.push(ReportActionLog {
        id: format!("action-{now}"),
        action: payload.action,
        operator_id: operator_id.to_string(),
        note: payload.note.clone().unwrap_or_default(),
        created_at: now,
        params: payload
            .params
            .as_ref()
            .and_then(|p| serde_json::to_value(p).ok()),
    });
    report.status = status;
    report.updated_at = now;
    // revoke 把源举报退回 pending 并清合并标记；其它动作保持当前 merged_by_punishment_id。
    let merged = if payload.action == ReportAction::Revoke {
        report.merged_by_punishment_id = None;
        None
    } else {
        report.merged_by_punishment_id.as_deref()
    };
    persistence
        .update_action(uuid, status, &report.action_log, merged, now)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_report() -> Report {
        Report {
            id: Uuid::new_v4().to_string(),
            reporter: None,
            reporter_id: Uuid::new_v4().to_string(),
            reporter_name: "举报人甲".to_string(),
            reporter_avatar: "/static/a.png".to_string(),
            target_type: ReportTargetType::PlayerBehavior,
            target_id: "room-ABC".to_string(),
            target_user: None,
            target_rule: None,
            reason: "言语辱骂".to_string(),
            details: "在对局中持续辱骂其他玩家".to_string(),
            status: ReportStatus::Pending,
            created_at: 1_700_000_000_000,
            updated_at: 1_700_000_000_000,
            context: Some(serde_json::json!({"targetLabel": "房间 ABC", "roomCode": "ABC"})),
            punishment: None,
            merged_by_punishment_id: None,
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
            params: None,
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
            params: None,
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
            params: None,
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

    #[test]
    fn can_ban_user_rejects_admin_target() {
        // 防误封：目标是 admin 必须返回 Forbidden。
        let err = can_ban_user("admin").expect_err("封禁 admin 应被拒绝");
        assert!(matches!(err, AppError::Forbidden(_)));
    }

    #[test]
    fn can_ban_user_allows_normal_user_target() {
        assert!(can_ban_user("user").is_ok());
        assert!(can_ban_user("").is_ok());
    }

    #[test]
    fn reporter_struct_and_flat_fields_both_serialize() {
        // FE 同时读 reporter 结构与扁平 reporterId/Name/Avatar，两套都必须出网。
        let mut report = sample_report();
        report.reporter = Some(ReportUser {
            id: report.reporter_id.clone(),
            name: report.reporter_name.clone(),
            avatar: report.reporter_avatar.clone(),
        });
        let json = serde_json::to_value(&report).unwrap();
        assert!(json.get("reporter").and_then(|v| v.get("id")).is_some());
        assert!(json.get("reporterId").and_then(|v| v.as_str()).is_some());
        assert!(json.get("reporterName").and_then(|v| v.as_str()).is_some());
        assert!(
            json.get("reporterAvatar")
                .and_then(|v| v.as_str())
                .is_some()
        );
    }

    #[test]
    fn target_user_id_derives_for_user_and_player_behavior() {
        let mut report = sample_report();
        // player_behavior 类用 target_id。
        assert_eq!(report_target_user_id(&report).as_deref(), Some("room-ABC"));
        // 结构化 target_user 优先。
        report.target_user = Some(ReportTargetUser {
            id: "user-99".to_string(),
            name: None,
            avatar: None,
        });
        assert_eq!(report_target_user_id(&report).as_deref(), Some("user-99"));
        // rule 类无 target_user 时无派生 user id。
        let mut rule_report = sample_report();
        rule_report.target_type = ReportTargetType::Rule;
        rule_report.context = None;
        assert_eq!(report_target_user_id(&rule_report), None);
    }

    #[test]
    fn target_rule_id_derives_from_context_and_target() {
        let mut report = sample_report();
        report.target_type = ReportTargetType::Rule;
        report.context = Some(serde_json::json!({"ruleId": "rule_ctx"}));
        // context.ruleId 优先于 target_id。
        assert_eq!(report_target_rule_id(&report).as_deref(), Some("rule_ctx"));
        // 无 context 时 rule 类用 target_id。
        report.context = None;
        report.target_id = "rule_tid".to_string();
        assert_eq!(report_target_rule_id(&report).as_deref(), Some("rule_tid"));
        // 结构化 target_rule 优先。
        report.target_rule = Some(ReportTargetRule {
            id: "rule_struct".to_string(),
            name: None,
            author_id: None,
            author_name: None,
        });
        assert_eq!(
            report_target_rule_id(&report).as_deref(),
            Some("rule_struct")
        );
    }

    #[test]
    fn target_user_and_rule_filters_match_structured_fields() {
        let mut report = sample_report();
        report.target_user = Some(ReportTargetUser {
            id: "user-7".to_string(),
            name: Some("Bob".to_string()),
            avatar: None,
        });
        assert!(report_matches_target_user(&report, "user-7"));
        assert!(report_matches_target_user(&report, "bob"));
        assert!(!report_matches_target_user(&report, "zzz"));
        // 空过滤视为全命中。
        assert!(report_matches_target_user(&report, ""));

        let mut rule_report = sample_report();
        rule_report.target_type = ReportTargetType::Rule;
        rule_report.target_rule = Some(ReportTargetRule {
            id: "rule_x".to_string(),
            name: Some("斗地主".to_string()),
            author_id: None,
            author_name: None,
        });
        assert!(report_matches_target_rule(&rule_report, "rule_x"));
        assert!(report_matches_target_rule(&rule_report, "斗地主"));
        assert!(!report_matches_target_rule(&rule_report, "麻将"));
    }

    #[test]
    fn action_params_roundtrip_camel_case() {
        let payload: ReportActionPayload = serde_json::from_value(serde_json::json!({
            "action": "ban_both",
            "note": "封号下架",
            "params": {
                "banDays": 7,
                "ruleAuthorId": "author-1",
                "targetRuleId": "rule_1"
            }
        }))
        .unwrap();
        assert_eq!(payload.action, ReportAction::BanBoth);
        let params = payload.params.unwrap();
        assert_eq!(params.ban_days, Some(7));
        assert_eq!(params.rule_author_id.as_deref(), Some("author-1"));
        assert_eq!(params.target_rule_id.as_deref(), Some("rule_1"));
    }
}
