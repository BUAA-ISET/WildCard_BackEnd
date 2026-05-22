use wildcard_backend::{
    TestRuleDefinition,
    api::TestApp,
    domain::{
        room::{GameRuleOption, Player, Room, RoomRuleResponse, RoomStatus},
        rule_engine::{ExportedRuleDesign, PlayerActionInput, RuleEngine},
    },
    websocket::RoomSession,
};

fn load_tiny_demo_rule() -> ExportedRuleDesign {
    let content = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test2.json"),
    )
    .expect("test2.json should exist");

    serde_json::from_str(&content).expect("test2.json should be valid rule json")
}

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
fn test_app_keeps_auth_state_until_logout() {
    let mut app = TestApp::new();

    assert_eq!(
        app.register_user_with_password("alice", "correct")
            .status_code,
        201,
    );
    assert_eq!(app.login_user("alice", "wrong").status_code, 401);
    assert_eq!(app.get_current_user(), None);

    assert_eq!(app.login_user("alice", "correct").status_code, 200);
    assert_eq!(app.get_current_user(), Some("alice"));

    assert_eq!(app.logout().status_code, 200);
    assert_eq!(app.get_current_user(), None);
}

#[test]
fn test_app_rejects_invalid_room_and_rule_inputs() {
    let mut app = TestApp::new();

    assert_eq!(app.create_room("   ").message, "room id is required");
    assert_eq!(app.create_room("ROOM1").status_code, 201);
    assert_eq!(app.create_room("ROOM1").message, "room already exists");

    let missing_body = app.validate_rule_definition("   ");
    assert_eq!(missing_body.status_code, 400);
    assert_eq!(missing_body.message, "rule payload is required");

    let invalid_body = app.validate_rule_definition(r#"{"player_count":2}"#);
    assert_eq!(invalid_body.status_code, 422);
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

#[test]
fn rule_engine_runs_tiny_demo_to_settlement_from_public_api() {
    let runtime_rule = RuleEngine::parse(
        "Tiny Demo".to_string(),
        2,
        "integration fixture".to_string(),
        load_tiny_demo_rule(),
    )
    .expect("tiny demo rule should compile");
    let mut session = RuleEngine::start_session(
        "room-finish".to_string(),
        &runtime_rule,
        vec!["player-a".to_string(), "player-b".to_string()],
    )
    .expect("session should start");

    let first_card = session.hands["player-a"][0].id.clone();
    RuleEngine::submit_action(
        &runtime_rule,
        &mut session,
        "player-a",
        PlayerActionInput {
            cards: vec![first_card],
            choice: None,
        },
    )
    .expect("valid single-card play should finish the tiny demo rule");

    assert_eq!(session.status, "finished");
    assert_eq!(session.settlement_results["player-a"], 1);
    assert_eq!(session.settlement_results["player-b"], 0);
}

#[test]
fn websocket_room_session_keeps_participants_sorted_and_idempotent() {
    let mut session = RoomSession::new("room-1");

    session.join("user-b");
    session.join("user-a");
    session.join("user-b");

    assert_eq!(session.participant_count(), 2);
    assert_eq!(session.participants(), vec!["user-a", "user-b"]);

    let event = session.leave("missing-user");
    assert_eq!(event.action, "left");
    assert_eq!(session.participant_count(), 2);
}

#[test]
fn fixture_rule_definition_round_trips_through_json() {
    let rule = TestRuleDefinition {
        id: "rule_001".to_string(),
        name: "basic_rule".to_string(),
        version: 1,
        steps: vec!["draw".to_string(), "discard".to_string()],
    };

    let encoded = serde_json::to_string(&rule).expect("rule should serialize");
    let decoded: TestRuleDefinition =
        serde_json::from_str(&encoded).expect("rule should deserialize");

    assert_eq!(decoded, rule);
}
