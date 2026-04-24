mod common;

use wildcard_backend::healthcheck;

#[test]
fn sample_fixture_is_stable() {
    let user = common::sample_user();

    assert_eq!(user.id, 1);
    assert_eq!(user.username, "test_user");
    assert_eq!(user.email, "test@example.com");
}

#[test]
fn backend_test_framework_is_available() {
    assert_eq!(healthcheck(), "ok");
}

#[test]
fn bundled_json_fixtures_can_be_loaded() {
    let rule = common::load_fixture::<wildcard_backend::TestRuleDefinition>("rule_sample.json");

    assert_eq!(rule.id, "rule_001");
    assert_eq!(rule.name, "basic_rule");
}
