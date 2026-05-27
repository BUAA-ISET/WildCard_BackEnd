use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::email::MailAddress;

#[derive(PartialEq, PartialOrd, Eq, Clone, Serialize, Deserialize, Debug)]
pub struct UserId(pub Uuid);

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub name: String,
    pub email: MailAddress,
    pub avatar: String,
    #[serde(skip_serializing)]
    pub password: String,
}
