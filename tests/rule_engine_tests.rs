use wildcard_backend::domain::rule_engine::{ExportedRuleDesign, PlayerActionInput, RuleEngine};

fn load_tiny_demo_rule() -> ExportedRuleDesign {
    let content = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test2.json"),
    )
    .expect("test2.json should exist");

    serde_json::from_str(&content).expect("test2.json should be valid rule json")
}

fn parse_tiny_demo_rule() -> wildcard_backend::domain::rule_engine::RuntimeRule {
    RuleEngine::parse(
        "Tiny Demo".to_string(),
        2,
        "integration fixture".to_string(),
        load_tiny_demo_rule(),
    )
    .expect("tiny demo rule should compile")
}

#[test]
fn parse_rejects_empty_rule_metadata() {
    let error = RuleEngine::parse(
        "   ".to_string(),
        2,
        "missing name".to_string(),
        load_tiny_demo_rule(),
    )
    .expect_err("blank rule names must be rejected");

    assert!(error.to_string().contains("规则名称不能为空"));
}

#[test]
fn parse_rejects_rules_without_card_class() {
    let mut design = load_tiny_demo_rule();
    design.classes.remove("card");
    let error = RuleEngine::parse(
        "No Cards".to_string(),
        2,
        "missing card class".to_string(),
        design,
    )
    .expect_err("rules need a card class to compile");

    assert!(error.to_string().contains("规则缺少固有类 classes.card"));
}

#[test]
fn submit_action_rejects_non_current_player() {
    let runtime_rule = parse_tiny_demo_rule();
    let mut session = RuleEngine::start_session(
        "room-turn".to_string(),
        &runtime_rule,
        vec!["player-a".to_string(), "player-b".to_string()],
    )
    .expect("session should start");

    let first_card = session.hands["player-b"][0].id.clone();
    let error = RuleEngine::submit_action(
        &runtime_rule,
        &mut session,
        "player-b",
        PlayerActionInput {
            cards: vec![first_card],
            choice: None,
        },
    )
    .expect_err("only the pending player may act");

    assert!(error.to_string().contains("还没有轮到该玩家操作"));
}

#[test]
fn submit_action_rejects_skip_before_any_successful_play() {
    let runtime_rule = parse_tiny_demo_rule();
    let mut session = RuleEngine::start_session(
        "room-skip".to_string(),
        &runtime_rule,
        vec!["player-a".to_string(), "player-b".to_string()],
    )
    .expect("session should start");

    let error = RuleEngine::submit_action(
        &runtime_rule,
        &mut session,
        "player-a",
        PlayerActionInput {
            cards: Vec::new(),
            choice: None,
        },
    )
    .expect_err("first player cannot skip before a card has been played");

    assert!(
        error
            .to_string()
            .contains("Cannot skip before any card has been played")
    );
}

#[test]
fn submit_action_accepts_valid_single_card_and_sets_winner() {
    let runtime_rule = parse_tiny_demo_rule();
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
            cards: vec![first_card.clone()],
            choice: None,
        },
    )
    .expect("valid single-card play should finish the tiny demo rule");

    assert_eq!(session.status, "finished");
    assert_eq!(session.last_action_player_id.as_deref(), Some("player-a"));
    assert_eq!(session.last_action_cards.len(), 1);
    assert_eq!(session.settlement_results["player-a"], 1);
    assert_eq!(session.settlement_results["player-b"], 0);
    assert!(
        !session.hands["player-a"]
            .iter()
            .any(|card| card.id == first_card)
    );
}
