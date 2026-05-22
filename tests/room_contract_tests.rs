use wildcard_backend::domain::room::{GameRuleOption, Player, Room, RoomRuleResponse, RoomStatus};
fn player(id: &str, is_ready: bool, joined_at: i64) -> Player {
    Player {
        id: id.to_string(),
        username: format!("user-{id}"),
        avatar: String::new(),
        is_ready,
        joined_at: Some(joined_at),
    }
}

#[test]
fn room_domain_serializes_public_room_shape() {
    let room = Room {
        id: "room-1".to_string(),
        code: "ROOM1".to_string(),
        host_id: "host".to_string(),
        player_count: 2,
        round_time: 30,
        rule_id: "rule-1".to_string(),
        rule_name: "Tiny Demo".to_string(),
        password: None,
        has_password: true,
        players: vec![player("host", true, 1), player("guest", false, 2)],
        status: RoomStatus::Waiting,
        game_session_id: None,
    };

    let value = serde_json::to_value(&room).expect("room should serialize");

    assert_eq!(value["hostId"], "host");
    assert!(value.get("host_id").is_none());
    assert_eq!(value["hasPassword"], true);
    assert_eq!(value["players"][0]["isReady"], true);
    assert_eq!(value["players"][0]["joinedAt"], 1);
    assert_eq!(value["status"], "waiting");
}

#[test]
fn room_rule_response_preserves_rule_payload() {
    let response = RoomRuleResponse {
        room_id: "room-1".to_string(),
        rule: serde_json::json!({
            "name": "Tiny Demo",
            "player_count": 2,
        }),
    };

    let value = serde_json::to_value(&response).expect("room rule response should serialize");

    assert_eq!(value["room_id"], "room-1");
    assert_eq!(value["rule"]["name"], "Tiny Demo");
    assert_eq!(value["rule"]["player_count"], 2);
}

#[test]
fn game_rule_option_uses_camel_case_player_count() {
    let option = GameRuleOption {
        id: "rule-1".to_string(),
        name: "Tiny Demo".to_string(),
        player_count: 2,
        description: Some("demo".to_string()),
    };

    let value = serde_json::to_value(&option).expect("rule option should serialize");

    assert_eq!(value["playerCount"], 2);
    assert!(value.get("player_count").is_none());
}
