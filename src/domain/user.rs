use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(PartialEq, PartialOrd, Eq, Clone, Serialize, Deserialize, Debug)]
pub struct UserId(pub Uuid);

impl ToString for UserId {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}

#[derive(Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub name: String,
    pub email: String,
    pub password: String,
}
