use wildcard_backend::TestRuleDefinition;

#[test]
fn fixture_rule_definition_round_trips_through_json() {
    let rule = TestRuleDefinition {
        id: "rule_001".to_string(),
        name: "basic_rule".to_string(),
        version: 1,
        steps: vec!["draw".to_string(), "discard".to_string()],
    };

    let encoded = serde_json::to_string(&rule).expect("rule should serialize");
    let decoded: TestRuleDefinition =
        serde_json::from_str(&encoded).expect("rule should deserialize");

    assert_eq!(decoded, rule);
}
