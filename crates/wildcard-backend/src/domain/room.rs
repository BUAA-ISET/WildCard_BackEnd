#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GameRuleOption {
    pub id: String,
    pub name: String,
    pub player_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Player {
    pub id: String,
    pub username: String,
    pub avatar: String,
    #[serde(rename = "isReady")]
    pub is_ready: bool,
    #[serde(rename = "joinedAt", skip_serializing_if = "Option::is_none")]
    pub joined_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RoomStatus {
    Waiting,
    Playing,
    Finished,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Room {
    pub id: String,
    pub code: String,
    pub host_id: String,
    pub player_count: usize,
    pub round_time: u32,
    pub rule_id: String,
    pub rule_name: String,
    pub password: Option<String>,
    #[serde(rename = "hasPassword")]
    pub has_password: bool,
    pub players: Vec<Player>,
    pub status: RoomStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoomRuleResponse {
    pub room_id: String,
    pub rule: Value,
}

#[derive(Debug, Clone)]
pub struct RuleCatalogEntry {
    pub option: GameRuleOption,
    pub definition: Value,
}

pub fn default_rule_catalog() -> HashMap<String, RuleCatalogEntry> {
    let mut catalog = HashMap::new();
    catalog.insert(
        "classic".to_string(),
        RuleCatalogEntry {
            option: GameRuleOption {
                id: "classic".to_string(),
                name: "Classic Rules".to_string(),
                player_count: 4,
                description: Some("4 players, standard wildcard flow.".to_string()),
            },
            definition: json!({
                "name": "standard-game",
                "player_count": 4,
                "classes": {
                    "card": {
                        "default_properties": {
                            "point": {
                                "config": [
                                    { "display": "3", "value": 3 },
                                    { "display": "4", "value": 4 },
                                    { "display": "5", "value": 5 },
                                    { "display": "6", "value": 6 },
                                    { "display": "7", "value": 7 },
                                    { "display": "8", "value": 8 },
                                    { "display": "9", "value": 9 },
                                    { "display": "10", "value": 10 },
                                    { "display": "J", "value": 11 },
                                    { "display": "Q", "value": 12 },
                                    { "display": "K", "value": 13 },
                                    { "display": "A", "value": 14 },
                                    { "display": "2", "value": 15 }
                                ]
                            }
                        }
                    }
                },
                "cardsets": {
                    "1": {
                        "name": "任意出牌",
                        "properties": {},
                        "build_flow": {
                            "1": {
                                "type": 28,
                                "content": {
                                    "result": 1,
                                    "properties": {}
                                }
                            }
                        },
                        "compare_flow": {
                            "1": {
                                "type": 30,
                                "content": {
                                    "result": 1
                                }
                            }
                        },
                        "successors": []
                    }
                },
                "match_flow": {},
                "end_flow": {}
            }),
        },
    );
    catalog.insert(
        "party".to_string(),
        RuleCatalogEntry {
            option: GameRuleOption {
                id: "party".to_string(),
                name: "Party Rules".to_string(),
                player_count: 6,
                description: Some("6 players, faster and more chaotic.".to_string()),
            },
            definition: json!({
                "name": "party-game",
                "player_count": 6,
                "classes": {
                    "card": {
                        "default_properties": {
                            "point": {
                                "config": [
                                    { "display": "3", "value": 3 },
                                    { "display": "4", "value": 4 },
                                    { "display": "5", "value": 5 },
                                    { "display": "6", "value": 6 },
                                    { "display": "7", "value": 7 },
                                    { "display": "8", "value": 8 },
                                    { "display": "9", "value": 9 },
                                    { "display": "10", "value": 10 },
                                    { "display": "J", "value": 11 },
                                    { "display": "Q", "value": 12 },
                                    { "display": "K", "value": 13 },
                                    { "display": "A", "value": 14 },
                                    { "display": "2", "value": 15 }
                                ]
                            }
                        }
                    }
                },
                "cardsets": {
                    "1": {
                        "name": "任意出牌",
                        "properties": {},
                        "build_flow": {
                            "1": {
                                "type": 28,
                                "content": {
                                    "result": 1,
                                    "properties": {}
                                }
                            }
                        },
                        "compare_flow": {
                            "1": {
                                "type": 30,
                                "content": {
                                    "result": 1
                                }
                            }
                        },
                        "successors": []
                    }
                },
                "match_flow": {},
                "end_flow": {}
            }),
        },
    );
    catalog
}
