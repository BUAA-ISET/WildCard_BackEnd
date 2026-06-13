#![allow(dead_code)]

use std::sync::Arc;

use axum::{Json, extract::State};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

mod error {
    pub use wildcard_backend::error::*;
}

mod domain {
    pub mod report {
        pub use wildcard_backend::domain::report::*;
    }

    pub mod user {
        pub use wildcard_backend::domain::user::*;
    }
}

mod infrastructure {
    pub mod user {
        use std::{collections::HashMap, sync::Arc};

        use tokio::sync::RwLock;
        use uuid::Uuid;

        use crate::{
            domain::user::{User, UserId},
            error::AppError,
        };

        struct StoredUser {
            id: Uuid,
            name: String,
            email: String,
            password: String,
            avatar: String,
            role: String,
            banned: bool,
        }

        impl StoredUser {
            fn from_user(user: User) -> Self {
                Self {
                    id: user.id.0,
                    name: user.name,
                    email: user.email,
                    password: user.password,
                    avatar: user.avatar,
                    role: user.role,
                    banned: user.banned,
                }
            }

            fn to_user(&self) -> User {
                User {
                    id: UserId(self.id),
                    name: self.name.clone(),
                    email: self.email.clone(),
                    password: self.password.clone(),
                    avatar: self.avatar.clone(),
                    role: self.role.clone(),
                    banned: self.banned,
                }
            }
        }

        #[derive(Default)]
        pub struct UserRepository {
            users: Arc<RwLock<HashMap<Uuid, StoredUser>>>,
        }

        impl UserRepository {
            pub fn with_users(users: Vec<User>) -> Self {
                Self {
                    users: Arc::new(RwLock::new(
                        users
                            .into_iter()
                            .map(|user| (user.id.0, StoredUser::from_user(user)))
                            .collect(),
                    )),
                }
            }

            pub async fn find_by_id(&self, user_id: &UserId) -> Result<Option<User>, AppError> {
                Ok(self
                    .users
                    .read()
                    .await
                    .get(&user_id.0)
                    .map(StoredUser::to_user))
            }

            pub async fn set_user_banned(
                &self,
                user_id: &UserId,
                banned: bool,
            ) -> Result<(), AppError> {
                if let Some(user) = self.users.write().await.get_mut(&user_id.0) {
                    user.banned = banned;
                }
                Ok(())
            }
        }
    }
}

mod interface {
    pub mod auth {
        use serde::{Deserialize, Serialize};

        use crate::domain::user::UserId;

        #[derive(Debug, Serialize, Deserialize)]
        pub struct TokenClaims {
            #[serde(rename = "sub")]
            pub user_id: UserId,
            pub iat: usize,
            pub exp: usize,
        }
    }

    pub mod rule {
        use serde::Serialize;
        use std::sync::Arc;

        use crate::{domain::user::UserId, error::AppError, infrastructure::user::UserRepository};

        #[derive(Debug, Serialize)]
        pub struct ApiResponse<T> {
            pub success: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub data: Option<T>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub message: Option<String>,
        }

        impl<T> ApiResponse<T> {
            pub(crate) fn success(data: T) -> Self {
                Self {
                    success: true,
                    data: Some(data),
                    message: None,
                }
            }
        }

        pub async fn ensure_admin(
            user_id: &UserId,
            user_repo: &Arc<UserRepository>,
        ) -> Result<(), AppError> {
            let Some(user) = user_repo.find_by_id(user_id).await? else {
                return Err(AppError::Forbidden("需要管理员权限".to_string()));
            };
            if user.role == "admin" {
                Ok(())
            } else {
                Err(AppError::Forbidden("需要管理员权限".to_string()))
            }
        }

        pub async fn ensure_not_banned(
            user_id: &UserId,
            user_repo: &Arc<UserRepository>,
        ) -> Result<(), AppError> {
            let user = user_repo
                .find_by_id(user_id)
                .await?
                .ok_or(AppError::Unauthorized("用户不存在".to_string()))?;
            if user.banned {
                return Err(AppError::Forbidden(
                    "账号已被封禁，无法执行该操作".to_string(),
                ));
            }
            Ok(())
        }

        pub fn can_ban_user(target_role: &str) -> Result<(), AppError> {
            if target_role == "admin" {
                return Err(AppError::Forbidden("不能封禁管理员账号".to_string()));
            }
            Ok(())
        }

        /// 最小化的已发布规则：报表联动只读 / 写 `banned` 字段。
        #[derive(Debug, Default, Clone)]
        pub struct PublishedRule {
            pub id: String,
            pub banned: bool,
        }

        #[derive(Debug, Default)]
        pub struct RuleRepository {
            pub published: std::collections::HashMap<String, PublishedRule>,
        }

        #[derive(Clone)]
        pub struct RulePersistence {
            pub pool: sqlx::PgPool,
        }

        impl RulePersistence {
            pub async fn set_rule_banned(
                &self,
                _rule_id: &str,
                _banned: bool,
            ) -> Result<(), AppError> {
                // 联动测试只覆盖到达此处前的分支（鉴权 / 防误封），不真正打 DB。
                Ok(())
            }
        }
    }

    pub mod report {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/interface/report.rs"
        ));
    }
}

mod state {
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use crate::interface::rule::RuleRepository;

    pub type RuleStore = Arc<RwLock<RuleRepository>>;
}

use domain::{
    report::{Report, ReportAction, ReportStatus, ReportTargetType},
    user::{User, UserId},
};
use error::AppError;
use infrastructure::user::UserRepository;
use interface::{
    auth::TokenClaims,
    report::{
        ReportActionPayload, ReportListQuery, ReportPersistence, SubmitReportPayload,
        action_report, get_report, report_matches_keyword, submit_report,
    },
};

fn claims(user_id: Uuid) -> TokenClaims {
    TokenClaims {
        user_id: UserId(user_id),
        iat: 0,
        exp: usize::MAX,
    }
}

fn user(id: Uuid, name: &str, role: &str) -> User {
    User {
        id: UserId(id),
        name: name.to_string(),
        email: format!("{name}@example.com"),
        password: "hashed".to_string(),
        avatar: format!("/static/avatars/{name}.png"),
        role: role.to_string(),
        banned: false,
    }
}

fn persistence() -> ReportPersistence {
    ReportPersistence {
        pool: PgPoolOptions::new()
            .connect_lazy("postgres://user:password@127.0.0.1:1/wildcard_test")
            .unwrap(),
    }
}

fn sample_report() -> Report {
    Report {
        id: Uuid::new_v4().to_string(),
        reporter_id: "reporter-1".to_string(),
        reporter_name: "Alice".to_string(),
        reporter_avatar: String::new(),
        target_type: ReportTargetType::PlayerBehavior,
        target_id: "room-42".to_string(),
        reason: "恶意拖延".to_string(),
        details: "最后一轮持续不操作".to_string(),
        status: ReportStatus::Pending,
        created_at: 1_700_000_000_000,
        updated_at: 1_700_000_000_000,
        context: Some(serde_json::json!({
            "targetLabel": "房间 42 的玩家 Bob",
            "roomCode": "42",
            "sourcePath": "/battle/room-42"
        })),
        action_log: vec![],
    }
}

#[test]
fn report_query_deserializes_frontend_filter_contract() {
    let query: ReportListQuery = serde_json::from_value(serde_json::json!({
        "status": "all",
        "targetType": "player_behavior",
        "keyword": "房间 42",
        "page": 3
    }))
    .unwrap();

    assert_eq!(query.status.as_deref(), Some("all"));
    assert_eq!(query.target_type.as_deref(), Some("player_behavior"));
    assert_eq!(query.keyword.as_deref(), Some("房间 42"));
    assert_eq!(query.page, Some(3));
}

#[test]
fn keyword_filter_matches_frontend_local_fallback_fields() {
    let report = sample_report();

    assert!(report_matches_keyword(&report, "ROOM-42"));
    assert!(report_matches_keyword(&report, "恶意"));
    assert!(report_matches_keyword(&report, "alice"));
    assert!(report_matches_keyword(&report, "玩家 bob"));
    assert!(!report_matches_keyword(&report, "unrelated"));
}

#[test]
fn report_action_payload_preserves_optional_params_for_frontend_contract() {
    let payload: ReportActionPayload = serde_json::from_value(serde_json::json!({
        "action": "ban_rule",
        "note": "封禁相关规则",
        "params": {"targetType": "rule", "targetId": "rule-1"}
    }))
    .unwrap();

    assert_eq!(payload.action, ReportAction::BanRule);
    assert_eq!(payload.note.as_deref(), Some("封禁相关规则"));
    assert_eq!(
        payload
            .params
            .as_ref()
            .and_then(|v| v.get("targetId"))
            .and_then(|v| v.as_str()),
        Some("rule-1")
    );
}

#[tokio::test]
async fn submit_report_rejects_blank_reason_before_database_write() {
    let reporter_id = Uuid::new_v4();
    let result = submit_report(
        claims(reporter_id),
        State(persistence()),
        State(Arc::new(UserRepository::with_users(vec![user(
            reporter_id,
            "reporter",
            "user",
        )]))),
        Json(SubmitReportPayload {
            reporter_id: "spoofed".to_string(),
            reporter_name: "fallback".to_string(),
            reporter_avatar: String::new(),
            target_type: ReportTargetType::Rule,
            target_id: "rule-1".to_string(),
            reason: "   ".to_string(),
            details: String::new(),
            context: None,
        }),
    )
    .await;

    assert!(
        matches!(result, Err(AppError::InvalidInput(message)) if message == "举报原因不能为空")
    );
}

#[tokio::test]
async fn submit_report_rejects_blank_target_before_database_write() {
    let reporter_id = Uuid::new_v4();
    let result = submit_report(
        claims(reporter_id),
        State(persistence()),
        State(Arc::new(UserRepository::with_users(vec![user(
            reporter_id,
            "reporter",
            "user",
        )]))),
        Json(SubmitReportPayload {
            reporter_id: "spoofed".to_string(),
            reporter_name: "fallback".to_string(),
            reporter_avatar: String::new(),
            target_type: ReportTargetType::Review,
            target_id: "  ".to_string(),
            reason: "垃圾内容".to_string(),
            details: String::new(),
            context: None,
        }),
    )
    .await;

    assert!(
        matches!(result, Err(AppError::InvalidInput(message)) if message == "举报对象不能为空")
    );
}

#[tokio::test]
async fn admin_report_detail_invalid_id_returns_not_found_without_database_lookup() {
    let admin_id = Uuid::new_v4();
    let result = get_report(
        claims(admin_id),
        State(persistence()),
        State(Arc::new(UserRepository::with_users(vec![user(
            admin_id, "admin", "admin",
        )]))),
        axum::extract::Path("counts".to_string()),
    )
    .await;

    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn non_admin_cannot_process_report_action_before_database_lookup() {
    let user_id = Uuid::new_v4();
    let result = action_report(
        claims(user_id),
        State(persistence()),
        State(Arc::new(UserRepository::with_users(vec![user(
            user_id, "regular", "user",
        )]))),
        State(Arc::new(tokio::sync::RwLock::new(
            interface::rule::RuleRepository::default(),
        ))),
        State(interface::rule::RulePersistence {
            pool: PgPoolOptions::new()
                .connect_lazy("postgres://user:password@127.0.0.1:1/wildcard_test")
                .unwrap(),
        }),
        axum::extract::Path(Uuid::new_v4().to_string()),
        Json(ReportActionPayload {
            action: ReportAction::Dismiss,
            note: Some("证据不足".to_string()),
            params: None,
        }),
    )
    .await;

    assert!(matches!(result, Err(AppError::Forbidden(message)) if message == "需要管理员权限"));
}
