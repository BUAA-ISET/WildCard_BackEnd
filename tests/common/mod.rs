use serde::de::DeserializeOwned;
use std::{fs, path::PathBuf};
use wildcard_backend::{TestRuleDefinition, TestUser};

#[allow(dead_code)]
pub fn sample_user() -> TestUser {
    TestUser {
        id: 1,
        username: String::from("test_user"),
        email: String::from("test@example.com"),
    }
}

#[allow(dead_code)]
pub fn sample_rule() -> TestRuleDefinition {
    TestRuleDefinition {
        id: String::from("rule_001"),
        name: String::from("basic_rule"),
        version: 1,
        steps: vec![String::from("draw"), String::from("discard")],
    }
}

#[allow(dead_code)]
pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test-fixtures")
        .join(name)
}

#[allow(dead_code)]
pub fn load_fixture<T: DeserializeOwned>(name: &str) -> T {
    let contents =
        fs::read_to_string(fixture_path(name)).expect("fixture file should exist in test-fixtures");

    serde_json::from_str(&contents).expect("fixture file should be valid JSON")
}
