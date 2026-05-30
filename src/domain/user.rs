use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    pub email: String,
    pub password: String,
    pub avatar: String,
    /// 取值 "user" / "admin"。用 String 而非 enum 是为了让 UserRepository 直接把 DB 的 VARCHAR 透传到上层，
    /// 不增加 SQL 解码层。校验集中在 ensure_admin / init.sql CHECK 约束里。
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String {
    "user".to_string()
}
