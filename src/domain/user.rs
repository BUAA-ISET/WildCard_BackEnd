use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(PartialEq, PartialOrd, Eq, Clone, Copy, Serialize, Deserialize, Debug)]
pub struct UserId(pub Uuid);

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl UserId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub name: String,
    pub email: String,
    pub password: String,
}

impl User {
    pub fn new(name: String, password: String, email: String) -> Self {
        Self {
            id: UserId::new(),
            name,
            email,
            password,
        }
    }
}
