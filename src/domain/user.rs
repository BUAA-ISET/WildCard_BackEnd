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
    /// 是否被封禁。封禁只标记不删数据，可解封。登录与写接口处校验此字段，
    /// 鉴权（TokenClaims）不查库以免每次请求多一次 DB 往返。
    #[serde(default)]
    pub banned: bool,
}

fn default_role() -> String {
    "user".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_roundtrips_banned_field() {
        let user = User {
            id: UserId(Uuid::new_v4()),
            name: "alice".to_string(),
            email: "a@b.com".to_string(),
            password: "hash".to_string(),
            avatar: String::new(),
            role: "user".to_string(),
            banned: true,
        };
        let json = serde_json::to_value(&user).unwrap();
        assert_eq!(json.get("banned").and_then(|v| v.as_bool()), Some(true));
        let back: User = serde_json::from_value(json).unwrap();
        assert!(back.banned);
    }

    #[test]
    fn user_banned_defaults_false_when_absent() {
        // 旧数据 / 旧 FE 可能不带 banned，应默认 false 而非反序列化失败。
        let json = serde_json::json!({
            "id": Uuid::new_v4(),
            "name": "bob",
            "email": "bob@b.com",
            "password": "hash",
            "avatar": "",
            "role": "user"
        });
        let user: User = serde_json::from_value(json).unwrap();
        assert!(!user.banned);
    }
}
