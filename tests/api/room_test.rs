use wildcard_backend::api::TestApp;

#[test]
fn room_creation_with_empty_id_returns_bad_request() {
    let mut app = TestApp::new();
    let response = app.create_room("");

    assert_eq!(response.status_code, 400);
    assert_eq!(response.message, "room id is required");
}

#[test]
fn room_creation_with_whitespace_id_returns_bad_request() {
    let mut app = TestApp::new();
    let response = app.create_room("   ");

    assert_eq!(response.status_code, 400);
    assert_eq!(response.message, "room id is required");
}

#[test]
fn room_creation_success_returns_created() {
    let mut app = TestApp::new();
    let response = app.create_room("room_001");

    assert_eq!(response.status_code, 201);
    assert!(response.message.contains("room_001"));
    assert!(response.message.contains("created"));
}

#[test]
fn room_creation_with_existing_id_returns_conflict() {
    let mut app = TestApp::new();

    let first_response = app.create_room("room_001");
    assert_eq!(first_response.status_code, 201);

    let second_response = app.create_room("room_001");
    assert_eq!(second_response.status_code, 409);
    assert_eq!(second_response.message, "room already exists");
}

#[test]
fn multiple_different_rooms_can_be_created() {
    let mut app = TestApp::new();

    let response1 = app.create_room("room_001");
    assert_eq!(response1.status_code, 201);

    let response2 = app.create_room("room_002");
    assert_eq!(response2.status_code, 201);
}
