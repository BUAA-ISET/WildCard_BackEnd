use std::collections::HashMap;
use wildcard_backend::domain::replay::{
    MatchHistoryRecord, MatchReplay, MatchResult, ReplayAction, ReplayCard, ReplayCardDisplay,
    ReplayFrame, ReplayPlayer,
};

fn sample_record() -> MatchHistoryRecord {
    MatchHistoryRecord {
        id: "replay-session-1".to_string(),
        session_id: "session-1".to_string(),
        room_code: "ROOM01".to_string(),
        rule_id: "rule-1".to_string(),
        rule_name: "Demo Rule".to_string(),
        started_at: "2026-05-29T12:00:00Z".to_string(),
        ended_at: "2026-05-29T12:05:00Z".to_string(),
        result: MatchResult::Draw,
        players: vec![
            ReplayPlayer {
                id: "player-1".to_string(),
                username: "Alice".to_string(),
                avatar: String::new(),
            },
            ReplayPlayer {
                id: "player-2".to_string(),
                username: "Bob".to_string(),
                avatar: String::new(),
            },
        ],
        winner_ids: vec!["player-1".to_string()],
    }
}

#[test]
fn replay_history_uses_frontend_camel_case_contract() {
    let value = serde_json::to_value(sample_record()).expect("record should serialize");

    assert_eq!(value["sessionId"], "session-1");
    assert_eq!(value["roomCode"], "ROOM01");
    assert_eq!(value["ruleName"], "Demo Rule");
    assert_eq!(value["winnerIds"][0], "player-1");
}

#[test]
fn replay_history_result_is_relative_to_viewer() {
    let record = sample_record();

    assert_eq!(
        record.clone().with_result_for_player("player-1").result,
        MatchResult::Win
    );
    assert_eq!(
        record.with_result_for_player("player-2").result,
        MatchResult::Lose
    );
}

#[test]
fn replay_history_result_draws_when_no_winner_exists() {
    let mut record = sample_record();
    record.winner_ids.clear();

    assert_eq!(
        record.with_result_for_player("player-2").result,
        MatchResult::Draw
    );
}

#[test]
fn replay_record_can_check_participating_players() {
    let record = sample_record();

    assert!(record.includes_player("player-1"));
    assert!(!record.includes_player("spectator-1"));
}

#[test]
fn replay_detail_uses_frontend_camel_case_contract() {
    let card = ReplayCard {
        id: "card-1".to_string(),
        properties: HashMap::from([("point".to_string(), 13), ("suit".to_string(), 0)]),
        display: ReplayCardDisplay {
            rank: "K".to_string(),
            suit: "S".to_string(),
        },
    };
    let replay = MatchReplay {
        record: sample_record(),
        frames: vec![ReplayFrame {
            index: 1,
            elapsed_seconds: 15,
            current_player_id: "player-2".to_string(),
            hands: HashMap::from([("player-1".to_string(), vec![card.clone()])]),
            table_cards: vec![card.clone()],
            action: Some(ReplayAction {
                player_id: "player-1".to_string(),
                action: "playCards".to_string(),
                cards: vec![card],
                message: "打出了 1 张牌".to_string(),
                turn: 2,
            }),
        }],
    };

    let value = serde_json::to_value(replay).expect("replay should serialize");

    assert_eq!(value["frames"][0]["elapsedSeconds"], 15);
    assert_eq!(value["frames"][0]["currentPlayerId"], "player-2");
    assert_eq!(value["frames"][0]["tableCards"][0]["display"]["rank"], "K");
    assert_eq!(value["frames"][0]["action"]["playerId"], "player-1");
}
