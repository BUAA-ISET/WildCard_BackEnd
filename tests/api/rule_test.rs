use super::{common, mock};
use wildcard_backend::{api::TestApp, TestRuleDefinition};

#[test]
fn rule_fixture_can_be_loaded_from_json() {
    let rule = common::load_fixture::<TestRuleDefinition>("rule_sample.json");

    assert_eq!(rule.id, "rule_001");
    assert_eq!(rule.name, "basic_rule");
}

#[test]
fn rule_validation_smoke_test_accepts_nonempty_payload() {
    let app = TestApp::new();
    let payload = mock::test_data::rule_payload_json();

    let response = app.validate_rule_definition(&payload);

    assert_eq!(response.status_code, 200);
}

#[test]
fn rule_validation_rejects_blank_payload() {
    let app = TestApp::new();

    let response = app.validate_rule_definition("");

    assert_eq!(response.status_code, 400);
}
