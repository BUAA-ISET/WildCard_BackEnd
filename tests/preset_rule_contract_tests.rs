use std::path::Path;

use wildcard_backend::domain::rule_engine::{
    ExportedRuleDesign, PlayerActionInput, RuleEngine, RuntimeRule,
};

struct PresetRuleCase {
    filename: &'static str,
    name: &'static str,
    player_count: u8,
    deal_count: usize,
    expected_cardsets: &'static [&'static str],
}

const PRESET_RULES: &[PresetRuleCase] = &[
    PresetRuleCase {
        filename: "war.json",
        name: "War 拼点战争",
        player_count: 2,
        deal_count: 5,
        expected_cardsets: &["Single"],
    },
    PresetRuleCase {
        filename: "nine_nine.json",
        name: "99 累加",
        player_count: 2,
        deal_count: 14,
        expected_cardsets: &["Single"],
    },
    PresetRuleCase {
        filename: "big_two.json",
        name: "大老二极简版",
        player_count: 2,
        deal_count: 8,
        expected_cardsets: &["Single", "Pair", "Triple", "Bomb"],
    },
    PresetRuleCase {
        filename: "blackjack.json",
        name: "21 点（伪版）",
        player_count: 2,
        deal_count: 3,
        expected_cardsets: &["Three"],
    },
];

fn load_design(filename: &str) -> ExportedRuleDesign {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(filename);
    let content = std::fs::read_to_string(path).expect("preset rule fixture should exist");

    serde_json::from_str(&content).expect("preset rule fixture should deserialize")
}

fn parse_case(case: &PresetRuleCase) -> RuntimeRule {
    RuleEngine::parse(
        case.name.to_string(),
        case.player_count,
        format!("{} preset contract", case.name),
        load_design(case.filename),
    )
    .expect("preset rule should compile")
}

fn player_ids() -> Vec<String> {
    vec!["player-a".to_string(), "player-b".to_string()]
}

fn pending_player_cards(
    session: &wildcard_backend::domain::rule_engine::GameSession,
    count: usize,
) -> Vec<String> {
    let pending = session
        .pending_action
        .as_ref()
        .expect("session should be waiting for a player action");
    session.hands[&pending.player_id]
        .iter()
        .take(count)
        .map(|card| card.id.clone())
        .collect()
}

#[test]
fn preset_rule_fixtures_compile_and_start_with_expected_contracts() {
    for case in PRESET_RULES {
        let runtime = parse_case(case);

        assert_eq!(runtime.name, case.name);
        assert_eq!(runtime.player_count, case.player_count);
        for cardset_name in case.expected_cardsets {
            assert!(
                runtime
                    .cardset_flows
                    .values()
                    .any(|cardset| cardset.name == *cardset_name),
                "{} should expose cardset {cardset_name}",
                case.filename
            );
        }

        let session = RuleEngine::start_session(
            format!("preset-contract-{}", case.filename),
            &runtime,
            player_ids(),
        )
        .expect("preset rule should start a playable session");

        assert_eq!(session.rule_name, case.name);
        assert_eq!(session.player_count, case.player_count);
        assert_eq!(session.status, "running");
        assert_eq!(
            session.pending_action.as_ref().map(|a| a.component_type),
            Some(21)
        );
        assert_eq!(session.hands["player-a"].len(), case.deal_count);
        assert_eq!(session.hands["player-b"].len(), case.deal_count);
    }
}

#[test]
fn war_preset_can_run_all_five_rounds_to_settlement() {
    let runtime = parse_case(&PRESET_RULES[0]);
    let mut session = RuleEngine::start_session("war-contract".to_string(), &runtime, player_ids())
        .expect("war preset should start");

    for _ in 0..10 {
        if session.status == "finished" {
            break;
        }
        let pending_player = session
            .pending_action
            .as_ref()
            .expect("war round should wait for the next player")
            .player_id
            .clone();
        let cards = pending_player_cards(&session, 1);

        RuleEngine::submit_action(
            &runtime,
            &mut session,
            &pending_player,
            PlayerActionInput {
                cards,
                choice: None,
            },
        )
        .expect("single-card war action should be accepted");
    }

    assert_eq!(session.status, "finished");
    assert_eq!(session.table["current_round"], 5);
    assert!(session.settlement_results.contains_key("player-a"));
    assert!(session.settlement_results.contains_key("player-b"));
    assert_eq!(session.hands["player-a"].len(), 0);
    assert_eq!(session.hands["player-b"].len(), 0);
}

#[test]
fn blackjack_preset_requires_three_cards_and_finishes_after_both_players_submit() {
    let runtime = parse_case(&PRESET_RULES[3]);
    let mut session =
        RuleEngine::start_session("blackjack-contract".to_string(), &runtime, player_ids())
            .expect("blackjack preset should start");

    let first_player = session
        .pending_action
        .as_ref()
        .expect("blackjack should wait for player-a")
        .player_id
        .clone();
    let too_few_cards = pending_player_cards(&session, 2);
    let error = RuleEngine::submit_action(
        &runtime,
        &mut session,
        &first_player,
        PlayerActionInput {
            cards: too_few_cards,
            choice: None,
        },
    )
    .expect_err("blackjack should reject non-three-card submissions");
    assert!(error.to_string().contains("不符合当前规则中的任何牌型"));

    let first_three_cards = pending_player_cards(&session, 3);
    RuleEngine::submit_action(
        &runtime,
        &mut session,
        &first_player,
        PlayerActionInput {
            cards: first_three_cards,
            choice: None,
        },
    )
    .expect("three-card blackjack action should be accepted");

    let second_player = session
        .pending_action
        .as_ref()
        .expect("blackjack should wait for player-b")
        .player_id
        .clone();
    let second_three_cards = pending_player_cards(&session, 3);
    RuleEngine::submit_action(
        &runtime,
        &mut session,
        &second_player,
        PlayerActionInput {
            cards: second_three_cards,
            choice: None,
        },
    )
    .expect("second blackjack action should finish the match");

    assert_eq!(session.status, "finished");
    assert!(session.table["p0_score"] > 0);
    assert!(session.table["p1_score"] > 0);
    assert!(session.settlement_results.contains_key("player-a"));
    assert!(session.settlement_results.contains_key("player-b"));
}
