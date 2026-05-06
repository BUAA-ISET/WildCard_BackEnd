use wildcard_backend::api::TestApp;

#[test]
fn find_existing_user_returns_success() {
    let mut app = TestApp::new();
    app.register_user("test_user");

    let response = app.find_user("test_user");
    assert_eq!(response.status_code, 200);
}

#[test]
fn find_nonexistent_user_returns_not_found() {
    let app = TestApp::new();

    let response = app.find_user("nonexistent");
    assert_eq!(response.status_code, 404);
}

#[test]
fn find_user_with_empty_username_returns_not_found() {
    let app = TestApp::new();

    let response = app.find_user("");
    assert_eq!(response.status_code, 404);
}

#[test]
fn find_user_with_whitespace_username_returns_not_found() {
    let app = TestApp::new();

    let response = app.find_user("   ");
    assert_eq!(response.status_code, 404);
}

#[test]
fn find_multiple_users() {
    let mut app = TestApp::new();
    app.register_user("user1");
    app.register_user("user2");
    app.register_user("user3");

    assert_eq!(app.find_user("user1").status_code, 200);
    assert_eq!(app.find_user("user2").status_code, 200);
    assert_eq!(app.find_user("user3").status_code, 200);
    assert_eq!(app.find_user("user4").status_code, 404);
}
