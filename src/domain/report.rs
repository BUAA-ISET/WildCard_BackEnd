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

/// 管理员处理动作：ban_user / ban_rule / dismiss，与前端 `ReportAction` 对齐。
/// 产品决策：后端只记状态，不真正执行封禁 / 下架。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportAction {
    BanUser,
    BanRule,
    Dismiss,
}

impl ReportAction {
    /// 动作 → 落地状态映射（纯函数，便于单测）：
    /// dismiss 视为驳回举报（rejected）；其余（ban_user / ban_rule）视为已处理（resolved）。
    pub fn resulting_status(self) -> ReportStatus {
        match self {
            ReportAction::Dismiss => ReportStatus::Rejected,
            ReportAction::BanUser | ReportAction::BanRule => ReportStatus::Resolved,
        }
    }
}

/// 单条处理记录，内嵌在 reports.action_log JSONB 数组里。
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
}

/// 举报完整对象。`context` 用原始 JSON 透传（FE 的 ReportContext 字段都是可选的，
/// 直接存 JSONB 不丢未知字段最稳）。时间戳是毫秒。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub id: String,
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
    #[serde(rename = "actionLog", default)]
    pub action_log: Vec<ReportActionLog>,
}
