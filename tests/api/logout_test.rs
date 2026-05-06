use wildcard_backend::api::TestApp;

#[test]
fn logout_when_not_logged_in_returns_success() {
    let mut app = TestApp::new();
    let response = app.logout();
    assert_eq!(response.status_code, 200);
}

#[test]
fn logout_when_logged_in_returns_success() {
    let mut app = TestApp::new();
    app.register_user("test_user");
    app.login_user("test_user", "password");
    assert_eq!(app.get_current_user(), Some("test_user"));

    let response = app.logout();
    assert_eq!(response.status_code, 200);
    assert_eq!(app.get_current_user(), None);
}

#[test]
fn logout_twice_returns_success() {
    let mut app = TestApp::new();
    app.register_user("test_user");
    app.login_user("test_user", "password");
    app.logout();

    let response = app.logout();
    assert_eq!(response.status_code, 200);
    assert_eq!(app.get_current_user(), None);
}

#[test]
fn logout_clears_current_user() {
    let mut app = TestApp::new();
    app.register_user("user1");
    app.register_user("user2");

    app.login_user("user1", "password");
    assert_eq!(app.get_current_user(), Some("user1"));

    app.logout();
    assert_eq!(app.get_current_user(), None);

    app.login_user("user2", "password");
    assert_eq!(app.get_current_user(), Some("user2"));
}
