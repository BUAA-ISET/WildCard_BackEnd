use wildcard_backend::api::TestApp;
#[test]
fn test_app_keeps_auth_state_until_logout() {
    let mut app = TestApp::new();

    assert_eq!(
        app.register_user_with_password("alice", "correct")
            .status_code,
        201,
    );
    assert_eq!(app.login_user("alice", "wrong").status_code, 401);
    assert_eq!(app.get_current_user(), None);

    assert_eq!(app.login_user("alice", "correct").status_code, 200);
    assert_eq!(app.get_current_user(), Some("alice"));

    assert_eq!(app.logout().status_code, 200);
    assert_eq!(app.get_current_user(), None);
}

#[test]
fn test_app_rejects_invalid_room_and_rule_inputs() {
    let mut app = TestApp::new();

    assert_eq!(app.create_room("   ").message, "room id is required");
    assert_eq!(app.create_room("ROOM1").status_code, 201);
    assert_eq!(app.create_room("ROOM1").message, "room already exists");

    let missing_body = app.validate_rule_definition("   ");
    assert_eq!(missing_body.status_code, 400);
    assert_eq!(missing_body.message, "rule payload is required");

    let invalid_body = app.validate_rule_definition(r#"{"player_count":2}"#);
    assert_eq!(invalid_body.status_code, 422);
}
