use crate::domain::user::UserId;

use super::rule_engine::{ExportedRuleDesign, RuntimeRule};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub enum RuleStatus {
    Draft,
    Published,
}

#[derive(PartialEq, PartialOrd, Eq, Clone, Serialize, Deserialize, Debug)]
pub struct RuleId(pub Uuid);

#[derive(Debug, Clone, Serialize)]
pub struct PublishedRule {
    pub id: RuleId,
    pub owner_id: UserId,
    pub name: String,
    pub player_count: u8,
    pub description: String,
    pub version: u32,
    pub design: ExportedRuleDesign,
    #[serde(skip_serializing)]
    pub runtime: RuntimeRule,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleDraft {
    pub id: RuleId,
    pub owner_id: UserId,
    pub name: String,
    pub player_count: u8,
    pub description: String,
    pub status: RuleStatus,
    pub design: ExportedRuleDesign,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_rule_id: Option<String>,
}
