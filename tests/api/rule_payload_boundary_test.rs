use wildcard_backend::api::TestApp;

#[test]
fn rule_payload_rejects_empty_or_whitespace_body() {
    let app = TestApp::new();

    let empty = app.validate_rule_definition("");
    let whitespace = app.validate_rule_definition("   ");

    assert_eq!(empty.status_code, 400);
    assert_eq!(empty.message, "rule payload is required");
    assert_eq!(whitespace.status_code, 400);
    assert_eq!(whitespace.message, "rule payload is required");
}

#[test]
fn rule_payload_rejects_json_without_name_field() {
    let app = TestApp::new();

    let response = app.validate_rule_definition(r#"{"player_count":2,"steps":[]}"#);

    assert_eq!(response.status_code, 422);
    assert_eq!(response.message, "rule payload is invalid");
}

#[test]
fn rule_payload_accepts_named_rule_definition() {
    let app = TestApp::new();

    let response = app.validate_rule_definition(r#"{"name":"Tiny Demo","player_count":2}"#);

    assert_eq!(response.status_code, 200);
    assert_eq!(response.message, "rule payload accepted");
}
