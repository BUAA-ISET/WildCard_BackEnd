use wildcard_backend::domain::rule_engine::{ExportedRuleDesign, PlayerActionInput, RuleEngine};

fn load_tiny_demo_rule() -> ExportedRuleDesign {
    let content = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test2.json"),
    )
    .expect("test2.json should exist");

    serde_json::from_str(&content).expect("test2.json should be valid rule json")
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
