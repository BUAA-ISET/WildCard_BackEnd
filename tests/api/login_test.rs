use wildcard_backend::api::TestApp;

#[test]
fn login_with_nonexistent_user_returns_unauthorized() {
    let mut app = TestApp::new();
    let response = app.login_user("nonexistent", "password");
    assert_eq!(response.status_code, 401);
}

#[test]
fn login_with_wrong_password_returns_unauthorized() {
    let mut app = TestApp::new();
    app.register_user("test_user");
    let response = app.login_user("test_user", "wrong_password");
    assert_eq!(response.status_code, 401);
}

#[test]
fn login_with_correct_password_returns_success() {
    let mut app = TestApp::new();
    app.register_user("test_user");
    let response = app.login_user("test_user", "password");
    assert_eq!(response.status_code, 200);
    assert_eq!(app.get_current_user(), Some("test_user"));
}

#[test]
fn login_with_empty_username_returns_bad_request() {
    let mut app = TestApp::new();
    let response = app.login_user("", "password");
    assert_eq!(response.status_code, 400);
}

#[test]
fn login_with_empty_password_returns_bad_request() {
    let mut app = TestApp::new();
    app.register_user("test_user");
    let response = app.login_user("test_user", "");
    assert_eq!(response.status_code, 400);
}

#[test]
fn login_with_whitespace_username_returns_bad_request() {
    let mut app = TestApp::new();
    let response = app.login_user("   ", "password");
    assert_eq!(response.status_code, 400);
}

#[test]
fn login_with_whitespace_password_returns_bad_request() {
    let mut app = TestApp::new();
    app.register_user("test_user");
    let response = app.login_user("test_user", "   ");
    assert_eq!(response.status_code, 400);
}
