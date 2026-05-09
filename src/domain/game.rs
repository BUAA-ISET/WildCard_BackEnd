use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GameCardDisplay {
    pub rank: String,
    pub suit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GameCard {
    pub id: String,
    pub properties: GameCardProperties,
    pub display: GameCardDisplay,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GameCardProperties {
    pub point: u8,
    pub suit: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GamePlayerState {
    pub id: String,
    pub username: String,
    pub avatar: String,
    pub hand_cards: Vec<GameCard>,
    pub finished_at_turn: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GamePlayerView {
    pub id: String,
    pub username: String,
    pub avatar: String,
    pub card_count: usize,
    pub online: bool,
    pub finished: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GameTableView {
    pub played_cards: Vec<GameCard>,
    pub pass_streak: usize,
    pub last_played_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GameActionRecord {
    pub player_id: String,
    pub action: String,
    pub cards: Vec<GameCard>,
    pub message: String,
    pub turn: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PendingActionType {
    PlayCards,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PendingAction {
    pub action_id: String,
    pub player_id: String,
    pub action_type: PendingActionType,
    pub can_skip: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GameStatus {
    Playing,
    Settling,
    Finished,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GameSession {
    pub session_id: String,
    pub room_code: String,
    pub room_id: String,
    pub rule_id: String,
    pub status: GameStatus,
    pub current_player_id: String,
    pub round_time: u32,
    pub turn: u32,
    pub players: Vec<GamePlayerState>,
    pub table: GameTableView,
    pub last_action: Option<GameActionRecord>,
    pub winner_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GameSnapshot {
    pub session_id: String,
    pub room_code: String,
    pub rule_id: String,
    pub status: GameStatus,
    pub current_player_id: String,
    pub round_time: u32,
    pub deadline_at: Option<i64>,
    pub players: Vec<GamePlayerView>,
    pub table: GameTableView,
    pub hand_cards: Vec<GameCard>,
    pub pending_action: Option<PendingAction>,
    pub last_action: Option<GameActionRecord>,
    pub winner_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlayCardsRequest {
    pub card_ids: Vec<String>,
}
