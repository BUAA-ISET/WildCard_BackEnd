use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReplayCardDisplay {
    pub rank: String,
    pub suit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReplayCard {
    pub id: String,
    pub properties: HashMap<String, i64>,
    pub display: ReplayCardDisplay,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReplayPlayer {
    pub id: String,
    pub username: String,
    pub avatar: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReplayAction {
    pub player_id: String,
    pub action: String,
    pub cards: Vec<ReplayCard>,
    pub message: String,
    pub turn: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReplayFrame {
    pub index: u32,
    pub elapsed_seconds: u32,
    pub current_player_id: String,
    pub hands: HashMap<String, Vec<ReplayCard>>,
    pub table_cards: Vec<ReplayCard>,
    pub action: Option<ReplayAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MatchResult {
    Win,
    Lose,
    Draw,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MatchHistoryRecord {
    pub id: String,
    pub session_id: String,
    pub room_code: String,
    pub rule_id: String,
    pub rule_name: String,
    pub started_at: String,
    pub ended_at: String,
    pub result: MatchResult,
    pub players: Vec<ReplayPlayer>,
    pub winner_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MatchReplay {
    pub record: MatchHistoryRecord,
    pub frames: Vec<ReplayFrame>,
}

impl MatchHistoryRecord {
    pub fn includes_player(&self, player_id: &str) -> bool {
        self.players.iter().any(|player| player.id == player_id)
    }

    pub fn with_result_for_player(mut self, player_id: &str) -> Self {
        self.result = if self.winner_ids.is_empty() {
            MatchResult::Draw
        } else if self
            .winner_ids
            .iter()
            .any(|winner_id| winner_id == player_id)
        {
            MatchResult::Win
        } else {
            MatchResult::Lose
        };
        self
    }
}
