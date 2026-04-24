use super::common;
use tokio_test::block_on;
use tungstenite::protocol::Message;
use wildcard_backend::websocket::RoomSession;

#[tokio::test]
async fn multi_client_room_session_tracks_join_and_leave() {
    let mut session = RoomSession::new("room_001");

    let first_join = session.join("alice");
    let second_join = session.join("bob");
    let first_leave = session.leave("alice");

    assert_eq!(first_join.action, "joined");
    assert_eq!(second_join.user_id, "bob");
    assert_eq!(first_leave.action, "left");
    assert_eq!(session.participant_count(), 1);

    let wire_message = Message::Text(
        format!(
            "{}:{}:{}",
            second_join.room_id, second_join.user_id, second_join.action
        )
        .into(),
    );

    assert!(matches!(wire_message, Message::Text(_)));
}

#[test]
fn tokio_test_can_drive_async_helpers_without_server() {
    let participants = block_on(async {
        let user = common::sample_user();
        let mut session = RoomSession::new("room_001");

        session.join(&user.username);
        session.participants()
    });

    assert_eq!(participants, vec![String::from("test_user")]);
}
