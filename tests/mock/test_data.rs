use wildcard_backend::{TestRuleDefinition, TestUser};

#[allow(dead_code)]
pub fn test_user() -> TestUser {
    TestUser {
        id: 1,
        username: String::from("test_user"),
        email: String::from("test@example.com"),
    }
}

#[allow(dead_code)]
pub fn register_user_payload() -> TestUser {
    TestUser {
        id: 2,
        username: String::from("new_user"),
        email: String::from("new@example.com"),
    }
}

#[allow(dead_code)]
pub fn rule_payload() -> TestRuleDefinition {
    TestRuleDefinition {
        id: String::from("rule_001"),
        name: String::from("basic_rule"),
        version: 1,
        steps: vec![String::from("draw"), String::from("discard")],
    }
}
