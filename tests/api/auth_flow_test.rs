use wildcard_backend::api::TestApp;

#[test]
fn login_requires_registered_user() {
    let mut app = TestApp::new();

    let response = app.login_user("missing-user", "password");

    assert_eq!(response.status_code, 401);
    assert_eq!(response.message, "user not found");
    assert!(app.get_current_user().is_none());
}

#[test]
fn login_rejects_wrong_password_without_setting_current_user() {
    let mut app = TestApp::new();
    assert_eq!(
        app.register_user_with_password("alice", "correct")
            .status_code,
        201
    );

    let response = app.login_user("alice", "wrong");

    assert_eq!(response.status_code, 401);
    assert_eq!(response.message, "invalid password");
    assert!(app.get_current_user().is_none());
}

#[test]
fn login_rejects_empty_credentials() {
    let mut app = TestApp::new();

    let empty_username = app.login_user("   ", "password");
    let empty_password = app.login_user("alice", "   ");

    assert_eq!(empty_username.status_code, 400);
    assert_eq!(empty_username.message, "username is required");
    assert_eq!(empty_password.status_code, 400);
    assert_eq!(empty_password.message, "password is required");
}

#[test]
fn logout_clears_current_user_after_successful_login() {
    let mut app = TestApp::new();
    assert_eq!(
        app.register_user_with_password("alice", "password")
            .status_code,
        201
    );
    assert_eq!(app.login_user("alice", "password").status_code, 200);
    assert_eq!(app.get_current_user(), Some("alice"));

    let response = app.logout();

    assert_eq!(response.status_code, 200);
    assert_eq!(response.message, "logged out");
    assert!(app.get_current_user().is_none());
}

#[test]
fn find_user_returns_not_found_for_missing_users() {
    let app = TestApp::new();

    let response = app.find_user("missing-user");

    assert_eq!(response.status_code, 404);
    assert_eq!(response.message, "user not found");
}
