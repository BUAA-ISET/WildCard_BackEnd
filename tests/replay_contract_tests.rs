use wildcard_backend::domain::replay::{
    MatchHistoryRecord, MatchResult, ReplayPlayer,
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
