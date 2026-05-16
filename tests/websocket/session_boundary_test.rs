use wildcard_backend::websocket::RoomSession;

#[test]
fn duplicate_join_does_not_increase_participant_count() {
    let mut session = RoomSession::new("room_001");

    session.join("user_001");
    let event = session.join("user_001");

    assert_eq!(event.room_id, "room_001");
    assert_eq!(event.user_id, "user_001");
    assert_eq!(event.action, "joined");
    assert_eq!(session.participant_count(), 1);
    assert_eq!(session.participants(), vec!["user_001"]);
}

#[test]
fn participants_are_returned_in_stable_order() {
    let mut session = RoomSession::new("room_001");

    session.join("user_003");
    session.join("user_001");
    session.join("user_002");

    assert_eq!(
        session.participants(),
        vec!["user_001", "user_002", "user_003"]
    );
}

#[test]
fn leaving_unknown_user_is_idempotent() {
    let mut session = RoomSession::new("room_001");
    session.join("user_001");

    let event = session.leave("missing_user");

    assert_eq!(event.room_id, "room_001");
    assert_eq!(event.user_id, "missing_user");
    assert_eq!(event.action, "left");
    assert_eq!(session.participant_count(), 1);
    assert_eq!(session.participants(), vec!["user_001"]);
}
