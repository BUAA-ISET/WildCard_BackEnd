use wildcard_backend::{TestRuleDefinition, TestUser};

pub fn register_user_payload() -> TestUser {
    TestUser {
        id: 1,
        username: String::from("test_user"),
        email: String::from("test@example.com"),
    }
}

pub fn room_identifier() -> &'static str {
    "room_001"
}

pub fn rule_payload() -> TestRuleDefinition {
    TestRuleDefinition {
        id: String::from("rule_001"),
        name: String::from("basic_rule"),
        version: 1,
        steps: vec![String::from("draw"), String::from("discard")],
    }
}

pub fn rule_payload_json() -> String {
    serde_json::to_string(&rule_payload()).expect("rule payload should serialize")
}
