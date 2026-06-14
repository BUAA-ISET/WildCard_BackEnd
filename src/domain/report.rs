use serde::{Deserialize, Serialize};

/// 举报对象类型。出网（FE / JSON）与入库（DB VARCHAR）都用 snake_case 字面值，
/// 与前端 `ReportTargetType` 联合类型逐字对齐：user / rule / review / player_behavior。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportTargetType {
    User,
    Rule,
    Review,
    PlayerBehavior,
}

impl ReportTargetType {
    pub fn as_str(self) -> &'static str {
        match self {
            ReportTargetType::User => "user",
            ReportTargetType::Rule => "rule",
            ReportTargetType::Review => "review",
            ReportTargetType::PlayerBehavior => "player_behavior",
        }
    }

    /// 从 DB / query 字符串解析；未知值返回 None（让上层决定是 400 还是忽略过滤）。
    pub fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "user" => Some(ReportTargetType::User),
            "rule" => Some(ReportTargetType::Rule),
            "review" => Some(ReportTargetType::Review),
            "player_behavior" => Some(ReportTargetType::PlayerBehavior),
            _ => None,
        }
    }
}

/// 举报处理状态：pending / resolved / rejected，与前端 `ReportStatus` 对齐。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportStatus {
    Pending,
    Resolved,
    Rejected,
}

impl ReportStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ReportStatus::Pending => "pending",
            ReportStatus::Resolved => "resolved",
            ReportStatus::Rejected => "rejected",
        }
    }

    /// 兜底到 Pending：未知值（脏数据 / 旧版本残留）按待处理处理，避免阻塞读取。
    pub fn from_db_str(value: &str) -> Self {
        match value {
            "resolved" => ReportStatus::Resolved,
            "rejected" => ReportStatus::Rejected,
            _ => ReportStatus::Pending,
        }
    }
}

/// 管理员处理动作，与前端 `ReportAction` 对齐：
/// ban_user / ban_rule / ban_both / dismiss / revoke。
/// ban_both 同时封用户 + 下架规则；revoke 撤销一条已落地的惩罚（完全逆转）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportAction {
    BanUser,
    BanRule,
    BanBoth,
    Dismiss,
    Revoke,
}

impl ReportAction {
    /// 出网 / 入库字面值（snake_case），与 serde 序列化一致。
    pub fn as_str(self) -> &'static str {
        match self {
            ReportAction::BanUser => "ban_user",
            ReportAction::BanRule => "ban_rule",
            ReportAction::BanBoth => "ban_both",
            ReportAction::Dismiss => "dismiss",
            ReportAction::Revoke => "revoke",
        }
    }

    /// 从 DB / 字符串解析；未知值返回 None。
    pub fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "ban_user" => Some(ReportAction::BanUser),
            "ban_rule" => Some(ReportAction::BanRule),
            "ban_both" => Some(ReportAction::BanBoth),
            "dismiss" => Some(ReportAction::Dismiss),
            "revoke" => Some(ReportAction::Revoke),
            _ => None,
        }
    }

    /// 动作 → 落地状态映射（纯函数，便于单测）：
    /// dismiss 视为驳回举报（rejected）；revoke 把举报退回 pending；
    /// ban_user / ban_rule / ban_both 视为已处理（resolved）。
    pub fn resulting_status(self) -> ReportStatus {
        match self {
            ReportAction::Dismiss => ReportStatus::Rejected,
            ReportAction::Revoke => ReportStatus::Pending,
            ReportAction::BanUser | ReportAction::BanRule | ReportAction::BanBoth => {
                ReportStatus::Resolved
            }
        }
    }
}

/// 惩罚作用域，与前端 `PunishmentScope` 对齐：user / rule / both。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PunishmentScope {
    User,
    Rule,
    Both,
}

impl PunishmentScope {
    pub fn as_str(self) -> &'static str {
        match self {
            PunishmentScope::User => "user",
            PunishmentScope::Rule => "rule",
            PunishmentScope::Both => "both",
        }
    }

    pub fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "user" => Some(PunishmentScope::User),
            "rule" => Some(PunishmentScope::Rule),
            "both" => Some(PunishmentScope::Both),
            _ => None,
        }
    }
}

/// 封禁判断纯函数：banned_until 为 None 视为未封禁；有值且大于 now 才算封禁中。
/// 解封 = 把 banned_until 置 None（不删数据，可逆）。
pub fn is_banned(banned_until: Option<i64>, now: i64) -> bool {
    banned_until.is_some_and(|until| until > now)
}

/// 动作 → 作用域映射（纯函数）：
/// ban_user → user，ban_rule → rule，ban_both → both，其余（dismiss / revoke）→ None。
pub fn action_scope(action: ReportAction) -> Option<PunishmentScope> {
    match action {
        ReportAction::BanUser => Some(PunishmentScope::User),
        ReportAction::BanRule => Some(PunishmentScope::Rule),
        ReportAction::BanBoth => Some(PunishmentScope::Both),
        ReportAction::Dismiss | ReportAction::Revoke => None,
    }
}

/// 合并判定纯函数，逐一对齐 FE `isSamePunishedTarget`：
/// 按 scope 判断候选举报是否与被罚目标同 user / 同 rule。空字符串视为"无目标"不匹配。
pub fn same_punished_target(
    scope: PunishmentScope,
    punished_user_id: Option<&str>,
    punished_rule_id: Option<&str>,
    candidate_user_id: Option<&str>,
    candidate_rule_id: Option<&str>,
) -> bool {
    let same_user = punished_user_id
        .filter(|id| !id.is_empty())
        .is_some_and(|id| candidate_user_id == Some(id));
    let same_rule = punished_rule_id
        .filter(|id| !id.is_empty())
        .is_some_and(|id| candidate_rule_id == Some(id));
    match scope {
        PunishmentScope::User => same_user,
        PunishmentScope::Rule => same_rule,
        PunishmentScope::Both => same_user || same_rule,
    }
}

/// 处理动作校验纯函数，逐一对齐 FE `validateLocalAction`。校验通过返回 Ok，
/// 否则返回与 FE 同口径的中文错误信息（上层包成 400）。
/// `has_rule_author`：targetRule.authorId 或 params.ruleAuthorId 任一存在即为 true。
pub fn validate_action(
    action: ReportAction,
    status: ReportStatus,
    has_active_punishment: bool,
    ban_days: Option<i64>,
    target_type: ReportTargetType,
    has_rule_author: bool,
) -> Result<(), String> {
    if action == ReportAction::Revoke {
        return if has_active_punishment {
            Ok(())
        } else {
            Err("当前举报没有可撤销的有效惩罚".to_string())
        };
    }
    if status != ReportStatus::Pending {
        return Err("该举报已处理，不能重复执行处罚".to_string());
    }
    if matches!(action, ReportAction::BanUser | ReportAction::BanBoth) && ban_days.is_none() {
        return Err("请选择用户封禁时长".to_string());
    }
    let rule_like = matches!(
        target_type,
        ReportTargetType::Rule | ReportTargetType::Review
    );
    if action == ReportAction::BanUser && rule_like && !has_rule_author {
        return Err("缺少规则作者 ID，无法封禁作者".to_string());
    }
    if action == ReportAction::BanBoth && !has_rule_author {
        return Err("缺少规则作者 ID，无法封禁作者".to_string());
    }
    Ok(())
}

/// 举报人 / 被举报用户 / 被举报规则的结构化对象，对齐 FE 的
/// `ReportUser` / `ReportTargetUser` / `ReportTargetRule`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReportUser {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub avatar: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportTargetUser {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportTargetRule {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "authorId", default, skip_serializing_if = "Option::is_none")]
    pub author_id: Option<String>,
    #[serde(
        rename = "authorName",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub author_name: Option<String>,
}

/// 惩罚记录，序列化字段逐一对齐 FE `ReportPunishment`（camelCase）。
/// report_id / target_user_id / target_rule_id 是 DB 内部字段（撤销时逆转用），
/// 不属于 FE 契约，跳过序列化。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Punishment {
    pub id: String,
    pub action: ReportAction,
    pub scope: PunishmentScope,
    pub active: bool,
    #[serde(rename = "banDays", default, skip_serializing_if = "Option::is_none")]
    pub ban_days: Option<i64>,
    #[serde(
        rename = "bannedUntil",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub banned_until: Option<i64>,
    #[serde(rename = "ruleRemoved")]
    pub rule_removed: bool,
    #[serde(rename = "affectedReportIds", default)]
    pub affected_report_ids: Vec<String>,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "revokedAt", default, skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<i64>,
    #[serde(skip)]
    pub report_id: String,
    #[serde(skip)]
    pub target_user_id: Option<String>,
    #[serde(skip)]
    pub target_rule_id: Option<String>,
}

/// 单条处理记录，内嵌在 reports.action_log JSONB 数组里。
/// `params` 透传 FE `ReportActionParams`（原始 JSON），便于审计回溯当时的调参。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportActionLog {
    pub id: String,
    pub action: ReportAction,
    #[serde(rename = "operatorId")]
    pub operator_id: String,
    #[serde(default)]
    pub note: String,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// 举报完整对象。`context` 用原始 JSON 透传（FE 的 ReportContext 字段都是可选的，
/// 直接存 JSONB 不丢未知字段最稳）。时间戳是毫秒。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub id: String,
    /// 结构化举报人对象，对齐 FE `reporter`。FE 同时保留旧扁平字段做 deprecated 兼容，
    /// 因此序列化时 reporter 结构 + reporterId/reporterName/reporterAvatar 两套都输出。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reporter: Option<ReportUser>,
    #[serde(rename = "reporterId")]
    pub reporter_id: String,
    #[serde(rename = "reporterName")]
    pub reporter_name: String,
    /// 空字符串视为"没有头像"：跳过序列化，对齐 FE 的 `reporterAvatar?`。
    #[serde(
        rename = "reporterAvatar",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub reporter_avatar: String,
    #[serde(rename = "targetType")]
    pub target_type: ReportTargetType,
    #[serde(rename = "targetId")]
    pub target_id: String,
    /// 结构化被举报用户 / 规则，对齐 FE `targetUser` / `targetRule`。读取时按需补齐。
    #[serde(
        rename = "targetUser",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub target_user: Option<ReportTargetUser>,
    #[serde(
        rename = "targetRule",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub target_rule: Option<ReportTargetRule>,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub details: String,
    pub status: ReportStatus,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
    /// 当前生效（或最近一次）的惩罚，读取时查 active punishment 补齐。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub punishment: Option<Punishment>,
    /// 被某条惩罚合并时记录该惩罚 ID，对齐 FE `mergedByPunishmentId`。
    #[serde(
        rename = "mergedByPunishmentId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub merged_by_punishment_id: Option<String>,
    #[serde(rename = "actionLog", default)]
    pub action_log: Vec<ReportActionLog>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_serde_ban_both_and_revoke() {
        assert_eq!(
            serde_json::to_value(ReportAction::BanBoth).unwrap(),
            serde_json::json!("ban_both")
        );
        assert_eq!(
            serde_json::to_value(ReportAction::Revoke).unwrap(),
            serde_json::json!("revoke")
        );
        let back: ReportAction = serde_json::from_value(serde_json::json!("ban_both")).unwrap();
        assert_eq!(back, ReportAction::BanBoth);
        let back: ReportAction = serde_json::from_value(serde_json::json!("revoke")).unwrap();
        assert_eq!(back, ReportAction::Revoke);
        assert_eq!(
            ReportAction::from_db_str("ban_both"),
            Some(ReportAction::BanBoth)
        );
        assert_eq!(
            ReportAction::from_db_str("revoke"),
            Some(ReportAction::Revoke)
        );
        assert_eq!(ReportAction::from_db_str("nope"), None);
        assert_eq!(ReportAction::BanBoth.as_str(), "ban_both");
        assert_eq!(ReportAction::Revoke.as_str(), "revoke");
    }

    #[test]
    fn revoke_action_maps_to_pending_status() {
        assert_eq!(
            ReportAction::Revoke.resulting_status(),
            ReportStatus::Pending
        );
        assert_eq!(
            ReportAction::BanBoth.resulting_status(),
            ReportStatus::Resolved
        );
    }

    #[test]
    fn punishment_serializes_camel_case() {
        let punishment = Punishment {
            id: "punishment-1".to_string(),
            action: ReportAction::BanBoth,
            scope: PunishmentScope::Both,
            active: true,
            ban_days: Some(7),
            banned_until: Some(1_700_000_604_800_000),
            rule_removed: true,
            affected_report_ids: vec!["r-2".to_string(), "r-3".to_string()],
            created_at: 1_700_000_000_000,
            revoked_at: None,
            report_id: "r-1".to_string(),
            target_user_id: Some("u-1".to_string()),
            target_rule_id: Some("rule_x".to_string()),
        };
        let json = serde_json::to_value(&punishment).unwrap();
        assert_eq!(json.get("banDays").and_then(|v| v.as_i64()), Some(7));
        assert_eq!(
            json.get("bannedUntil").and_then(|v| v.as_i64()),
            Some(1_700_000_604_800_000)
        );
        assert_eq!(
            json.get("ruleRemoved").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            json.get("affectedReportIds")
                .and_then(|v| v.as_array())
                .map(|a| a.len()),
            Some(2)
        );
        assert_eq!(
            json.get("createdAt").and_then(|v| v.as_i64()),
            Some(1_700_000_000_000)
        );
        // revokedAt 为 None 时不输出；DB 内部字段不出网。
        assert!(json.get("revokedAt").is_none());
        assert!(json.get("reportId").is_none());
        assert!(json.get("targetUserId").is_none());
        assert_eq!(
            json.get("action").and_then(|v| v.as_str()),
            Some("ban_both")
        );
        assert_eq!(json.get("scope").and_then(|v| v.as_str()), Some("both"));
    }

    #[test]
    fn is_banned_respects_until_vs_now() {
        let now = 1_700_000_000_000;
        assert!(is_banned(Some(now + 1), now));
        assert!(!is_banned(Some(now - 1), now));
        assert!(!is_banned(Some(now), now));
        assert!(!is_banned(None, now));
    }

    #[test]
    fn action_scope_maps_three_ban_actions() {
        assert_eq!(
            action_scope(ReportAction::BanUser),
            Some(PunishmentScope::User)
        );
        assert_eq!(
            action_scope(ReportAction::BanRule),
            Some(PunishmentScope::Rule)
        );
        assert_eq!(
            action_scope(ReportAction::BanBoth),
            Some(PunishmentScope::Both)
        );
        assert_eq!(action_scope(ReportAction::Dismiss), None);
        assert_eq!(action_scope(ReportAction::Revoke), None);
    }

    #[test]
    fn same_punished_target_matches_per_scope() {
        // scope=user：只看 user 命中。
        assert!(same_punished_target(
            PunishmentScope::User,
            Some("u-1"),
            Some("rule-1"),
            Some("u-1"),
            Some("rule-other"),
        ));
        assert!(!same_punished_target(
            PunishmentScope::User,
            Some("u-1"),
            Some("rule-1"),
            Some("u-2"),
            Some("rule-1"),
        ));
        // scope=rule：只看 rule 命中。
        assert!(same_punished_target(
            PunishmentScope::Rule,
            Some("u-1"),
            Some("rule-1"),
            Some("u-2"),
            Some("rule-1"),
        ));
        // scope=both：user 或 rule 任一命中。
        assert!(same_punished_target(
            PunishmentScope::Both,
            Some("u-1"),
            None,
            Some("u-1"),
            None,
        ));
        assert!(same_punished_target(
            PunishmentScope::Both,
            None,
            Some("rule-1"),
            None,
            Some("rule-1"),
        ));
        // 空 / None 目标不匹配。
        assert!(!same_punished_target(
            PunishmentScope::User,
            Some(""),
            None,
            Some(""),
            None,
        ));
        assert!(!same_punished_target(
            PunishmentScope::User,
            None,
            None,
            Some("u-1"),
            None,
        ));
    }

    #[test]
    fn validate_action_revoke_requires_active_punishment() {
        assert!(
            validate_action(
                ReportAction::Revoke,
                ReportStatus::Resolved,
                true,
                None,
                ReportTargetType::User,
                false,
            )
            .is_ok()
        );
        let err = validate_action(
            ReportAction::Revoke,
            ReportStatus::Resolved,
            false,
            None,
            ReportTargetType::User,
            false,
        )
        .unwrap_err();
        assert_eq!(err, "当前举报没有可撤销的有效惩罚");
    }

    #[test]
    fn validate_action_ban_user_requires_ban_days() {
        let err = validate_action(
            ReportAction::BanUser,
            ReportStatus::Pending,
            false,
            None,
            ReportTargetType::User,
            true,
        )
        .unwrap_err();
        assert_eq!(err, "请选择用户封禁时长");
        assert!(
            validate_action(
                ReportAction::BanUser,
                ReportStatus::Pending,
                false,
                Some(7),
                ReportTargetType::User,
                true,
            )
            .is_ok()
        );
    }

    #[test]
    fn validate_action_rejects_non_pending_for_punishment() {
        let err = validate_action(
            ReportAction::BanRule,
            ReportStatus::Resolved,
            false,
            None,
            ReportTargetType::Rule,
            true,
        )
        .unwrap_err();
        assert_eq!(err, "该举报已处理，不能重复执行处罚");
    }

    #[test]
    fn validate_action_ban_user_on_rule_needs_author() {
        let err = validate_action(
            ReportAction::BanUser,
            ReportStatus::Pending,
            false,
            Some(3),
            ReportTargetType::Rule,
            false,
        )
        .unwrap_err();
        assert_eq!(err, "缺少规则作者 ID，无法封禁作者");
        // review 类同理。
        let err = validate_action(
            ReportAction::BanUser,
            ReportStatus::Pending,
            false,
            Some(3),
            ReportTargetType::Review,
            false,
        )
        .unwrap_err();
        assert_eq!(err, "缺少规则作者 ID，无法封禁作者");
        // 有作者则通过。
        assert!(
            validate_action(
                ReportAction::BanUser,
                ReportStatus::Pending,
                false,
                Some(3),
                ReportTargetType::Rule,
                true,
            )
            .is_ok()
        );
    }

    #[test]
    fn validate_action_ban_both_always_needs_author() {
        let err = validate_action(
            ReportAction::BanBoth,
            ReportStatus::Pending,
            false,
            Some(5),
            ReportTargetType::User,
            false,
        )
        .unwrap_err();
        assert_eq!(err, "缺少规则作者 ID，无法封禁作者");
    }

    #[test]
    fn punishment_scope_db_roundtrip() {
        for s in [
            PunishmentScope::User,
            PunishmentScope::Rule,
            PunishmentScope::Both,
        ] {
            assert_eq!(PunishmentScope::from_db_str(s.as_str()), Some(s));
        }
        assert_eq!(PunishmentScope::from_db_str("garbage"), None);
    }
}
