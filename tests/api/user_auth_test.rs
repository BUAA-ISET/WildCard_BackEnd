use super::mock;
use wildcard_backend::api::TestApp;

#[test]
fn user_registration_with_empty_username_returns_bad_request() {
    let mut app = TestApp::new();
    let response = app.register_user("");

    assert_eq!(response.status_code, 400);
    assert_eq!(response.message, "username is required");
}

#[test]
fn user_registration_with_whitespace_username_returns_bad_request() {
    let mut app = TestApp::new();
    let response = app.register_user("   ");

    assert_eq!(response.status_code, 400);
    assert_eq!(response.message, "username is required");
}

#[test]
fn user_registration_with_existing_username_returns_conflict() {
    let mut app = TestApp::new();
    let payload = mock::test_data::register_user_payload();

    let first_response = app.register_user(&payload.username);
    assert_eq!(first_response.status_code, 201);

    let second_response = app.register_user(&payload.username);
    assert_eq!(second_response.status_code, 409);
    assert_eq!(second_response.message, "user already exists");
}

#[test]
fn user_registration_success_returns_created_with_message() {
    let mut app = TestApp::new();
    let payload = mock::test_data::register_user_payload();

    let response = app.register_user(&payload.username);

    assert_eq!(response.status_code, 201);
    assert!(response.message.contains(&payload.username));
    assert!(response.message.contains("created"));
}

#[test]
fn multiple_different_users_can_register_successfully() {
    let mut app = TestApp::new();

    let user1 = mock::test_data::register_user_payload();
    let response1 = app.register_user(&user1.username);
    assert_eq!(response1.status_code, 201);

    let mut user2 = mock::test_data::register_user_payload();
    user2.username = String::from("another_user");
    let response2 = app.register_user(&user2.username);
    assert_eq!(response2.status_code, 201);
}

#[test]
fn user_payload_fields_are_correct() {
    let user = mock::test_data::test_user();

    assert_eq!(user.username, "test_user");
    assert_eq!(user.email, "test@example.com");
    assert_eq!(user.id, 1);
}

#[test]
fn test_app_starts_with_empty_users() {
    let mut app = TestApp::new();
    let response = app.register_user("any_user");
    assert_eq!(response.status_code, 201);
}
