use wildcard_backend::websocket::{RoomEvent, RoomSession};

#[test]
fn room_session_can_be_created() {
    let session = RoomSession::new("room_001");

    assert_eq!(session.participant_count(), 0);
    assert!(session.participants().is_empty());
}

#[test]
fn user_can_join_room() {
    let mut session = RoomSession::new("room_001");
    let event = session.join("user_001");

    assert_eq!(event.room_id, "room_001");
    assert_eq!(event.user_id, "user_001");
    assert_eq!(event.action, "joined");
    assert_eq!(session.participant_count(), 1);
    assert_eq!(session.participants(), vec!["user_001"]);
}

#[test]
fn user_can_leave_room() {
    let mut session = RoomSession::new("room_001");
    session.join("user_001");

    let event = session.leave("user_001");

    assert_eq!(event.room_id, "room_001");
    assert_eq!(event.user_id, "user_001");
    assert_eq!(event.action, "left");
    assert_eq!(session.participant_count(), 0);
}

#[test]
fn multiple_users_can_join_same_room() {
    let mut session = RoomSession::new("room_001");

    let event1 = session.join("user_001");
    assert_eq!(event1.action, "joined");

    let event2 = session.join("user_002");
    assert_eq!(event2.action, "joined");

    assert_eq!(session.participant_count(), 2);
    assert_eq!(session.participants(), vec!["user_001", "user_002"]);
}

#[test]
fn user_can_join_and_leave_multiple_times() {
    let mut session = RoomSession::new("room_001");

    session.join("user_001");
    session.leave("user_001");
    let event = session.join("user_001");

    assert_eq!(event.action, "joined");
    assert_eq!(session.participant_count(), 1);
}

#[test]
fn room_event_clone_works() {
    let mut session = RoomSession::new("room_001");
    let event1 = session.join("user_001");
    let event2 = event1.clone();

    assert_eq!(event1.room_id, event2.room_id);
    assert_eq!(event1.user_id, event2.user_id);
    assert_eq!(event1.action, event2.action);
}