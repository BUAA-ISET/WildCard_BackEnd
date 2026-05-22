use wildcard_backend::websocket::RoomSession;

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
