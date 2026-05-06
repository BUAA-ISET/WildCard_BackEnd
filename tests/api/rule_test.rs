use super::common;
use wildcard_backend::api::TestApp;

#[test]
fn rule_validation_with_empty_body_returns_bad_request() {
    let app = TestApp::new();
    let response = app.validate_rule_definition("");

    assert_eq!(response.status_code, 400);
    assert_eq!(response.message, "rule payload is required");
}

#[test]
fn rule_validation_with_valid_payload_returns_accepted() {
    let app = TestApp::new();
    let rule = common::sample_rule();
    let payload = serde_json::to_string(&rule).expect("rule should serialize");

    let response = app.validate_rule_definition(&payload);

    assert_eq!(response.status_code, 200);
    assert_eq!(response.message, "rule payload accepted");
}

#[test]
fn rule_validation_with_invalid_payload_returns_unprocessable() {
    let app = TestApp::new();
    let response = app.validate_rule_definition("invalid json");

    assert_eq!(response.status_code, 422);
    assert_eq!(response.message, "rule payload is invalid");
}

#[test]
fn rule_validation_without_name_field_returns_unprocessable() {
    let app = TestApp::new();
    let payload = r#"{"id": "rule_001", "version": 1}"#;

    let response = app.validate_rule_definition(payload);

    assert_eq!(response.status_code, 422);
    assert_eq!(response.message, "rule payload is invalid");
}

#[test]
fn rule_fixture_can_be_loaded_from_json() {
    let rule = common::load_fixture::<wildcard_backend::TestRuleDefinition>("rule_sample.json");

    assert_eq!(rule.id, "rule_001");
    assert_eq!(rule.name, "basic_rule");
    assert_eq!(rule.version, 1);
    assert_eq!(rule.steps, vec!["draw", "discard"]);
}
