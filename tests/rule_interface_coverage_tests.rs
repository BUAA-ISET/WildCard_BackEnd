#![allow(dead_code)]
#![allow(deprecated)]

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use axum::{Json, extract::Path, extract::State};
use sqlx::postgres::PgPoolOptions;
use tokio::sync::RwLock;
use uuid::Uuid;

mod error {
    pub use wildcard_backend::error::*;
}

mod domain {
    pub mod user {
        pub use wildcard_backend::domain::user::*;
    }

    pub mod rule_engine {
        pub use wildcard_backend::domain::rule_engine::*;
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

        #[derive(Clone, Debug)]
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
            fn into_user(self) -> User {
                User {
                    id: UserId(self.id),
                    name: self.name,
                    email: self.email,
                    password: self.password,
                    avatar: self.avatar,
                    role: self.role,
                    banned: self.banned,
                }
            }
        }

        #[derive(Debug, Default)]
        pub struct UserRepository {
            users: Arc<RwLock<HashMap<Uuid, StoredUser>>>,
        }

        impl UserRepository {
            pub fn with_users(users: Vec<User>) -> Self {
                Self {
                    users: Arc::new(RwLock::new(
                        users
                            .into_iter()
                            .map(|user| {
                                (
                                    user.id.0,
                                    StoredUser {
                                        id: user.id.0,
                                        name: user.name,
                                        email: user.email,
                                        password: user.password,
                                        avatar: user.avatar,
                                        role: user.role,
                                        banned: user.banned,
                                    },
                                )
                            })
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
                    .cloned()
                    .map(StoredUser::into_user))
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

    pub mod user {
        pub fn extension_for_mime(mime: &str) -> Option<&'static str> {
            match mime {
                "image/png" => Some("png"),
                "image/jpeg" | "image/jpg" => Some("jpg"),
                "image/webp" => Some("webp"),
                _ => None,
            }
        }
    }

    pub mod rule {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/interface/rule.rs"
        ));
    }
}

mod state {
    use std::{path::PathBuf, sync::Arc};

    use tokio::sync::RwLock;

    use crate::interface::rule::RuleRepository;

    pub type RuleStore = Arc<RwLock<RuleRepository>>;

    #[derive(Clone)]
    pub struct UploadDir(pub Arc<PathBuf>);

    impl UploadDir {
        pub fn from_path(path: PathBuf) -> Self {
            Self(Arc::new(path))
        }
    }
}

use domain::{
    rule_engine::{ExportedRuleDesign, RuleEngine},
    user::{User, UserId},
};
use error::AppError;
use infrastructure::user::UserRepository;
use interface::{
    auth::TokenClaims,
    rule::{
        self, ForkRuleRequest, PublishedRule, RuleDraft, RulePersistence, RuleRepository,
        RuleStatus, SaveRuleDraftRequest,
    },
};
use state::RuleStore;

fn claims(user_id: Uuid) -> TokenClaims {
    TokenClaims {
        user_id: UserId(user_id),
        iat: 0,
        exp: usize::MAX,
    }
}

fn empty_design() -> ExportedRuleDesign {
    ExportedRuleDesign {
        classes: HashMap::new(),
        cardsets: HashMap::new(),
        cardset_comparisons: HashMap::new(),
        match_flow: HashMap::new(),
        end_flow: HashMap::new(),
        assets: Default::default(),
    }
}

fn valid_design() -> ExportedRuleDesign {
    let content =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("war.json"))
            .expect("war fixture should exist");
    serde_json::from_str(&content).expect("war fixture should parse")
}

fn published_rule(id: &str, name: &str) -> PublishedRule {
    let design = valid_design();
    let runtime = RuleEngine::parse(name.to_string(), 2, "desc".to_string(), design.clone())
        .expect("fixture rule should compile");
    PublishedRule {
        id: id.to_string(),
        owner_id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        player_count: 2,
        description: "desc".to_string(),
        version: 1,
        design,
        runtime,
        created_at: 1,
        updated_at: 1,
        introduction: "intro".to_string(),
        cover_url: "/static/rule-images/cover.png".to_string(),
        screenshot_urls: vec!["/static/rule-images/shot.png".to_string()],
        banned: false,
    }
}

fn draft(owner_id: Uuid, id: &str, status: RuleStatus, updated_at: i64) -> RuleDraft {
    RuleDraft {
        id: id.to_string(),
        owner_id: owner_id.to_string(),
        name: format!("draft-{id}"),
        player_count: 2,
        description: "desc".to_string(),
        status,
        design: valid_design(),
        created_at: 1,
        updated_at,
        published_rule_id: None,
        forked_from_rule_id: None,
        reject_reason: None,
        introduction: "intro".to_string(),
        cover_url: String::new(),
        screenshot_urls: Vec::new(),
    }
}

fn store_with(drafts: Vec<RuleDraft>, published: Vec<PublishedRule>) -> RuleStore {
    Arc::new(RwLock::new(RuleRepository {
        drafts: drafts
            .into_iter()
            .map(|draft| (draft.id.clone(), draft))
            .collect(),
        published: published
            .into_iter()
            .map(|rule| (rule.id.clone(), rule))
            .collect(),
    }))
}

fn persistence() -> RulePersistence {
    RulePersistence {
        pool: PgPoolOptions::new()
            .connect_lazy("postgres://user:password@localhost/wildcard_test")
            .unwrap(),
    }
}

/// 给写接口的 ban 校验用：一个含指定 user（默认未封禁）的内存 repo。
fn repo_with(user_id: Uuid) -> Arc<UserRepository> {
    repo_with_state(user_id, "user", false)
}

fn repo_with_state(user_id: Uuid, role: &str, banned: bool) -> Arc<UserRepository> {
    Arc::new(UserRepository::with_users(vec![User {
        id: UserId(user_id),
        name: "writer".to_string(),
        email: "writer@example.com".to_string(),
        password: "hashed".to_string(),
        avatar: String::new(),
        role: role.to_string(),
        banned,
    }]))
}

#[tokio::test]
async fn rule_draft_read_paths_filter_sort_and_check_ownership() {
    let owner = Uuid::new_v4();
    let other = Uuid::new_v4();
    let mut old = draft(owner, "old", RuleStatus::Draft, 10);
    old.published_rule_id = Some("rule-old".to_string());
    old.reject_reason = Some("needs work".to_string());
    let new = draft(owner, "new", RuleStatus::Rejected, 20);
    let hidden = draft(other, "hidden", RuleStatus::Draft, 30);
    let store = store_with(vec![old, new, hidden], Vec::new());

    let Json(response) = rule::list_drafts(claims(owner), State(store.clone()))
        .await
        .unwrap();
    let drafts = response.data.unwrap();
    assert_eq!(
        drafts
            .iter()
            .map(|draft| draft.id.as_str())
            .collect::<Vec<_>>(),
        vec!["new", "old"]
    );
    assert_eq!(drafts[1].published_rule_id.as_deref(), Some("rule-old"));
    assert_eq!(drafts[1].reject_reason.as_deref(), Some("needs work"));

    let Json(response) =
        rule::get_draft(claims(owner), State(store.clone()), Path("old".to_string()))
            .await
            .unwrap();
    assert_eq!(response.data.unwrap().id, "old");

    let unauthorized =
        rule::get_draft(claims(other), State(store.clone()), Path("old".to_string()))
            .await
            .unwrap_err();
    assert!(matches!(unauthorized, AppError::Unauthorized(_)));

    let missing = rule::get_draft(claims(owner), State(store), Path("missing".to_string()))
        .await
        .unwrap_err();
    assert!(matches!(missing, AppError::NotFound));
}

#[tokio::test]
async fn rule_write_paths_exercise_early_errors_without_database() {
    let owner = Uuid::new_v4();
    let other = Uuid::new_v4();
    let mut pending = draft(owner, "pending", RuleStatus::PendingReview, 88);
    pending.reject_reason = Some("old reason".to_string());
    let mut invalid = draft(owner, "invalid", RuleStatus::Draft, 1);
    invalid.name = "   ".to_string();
    let store = store_with(vec![pending, invalid], Vec::new());

    let Json(response) = rule::submit_review(
        claims(owner),
        State(store.clone()),
        State(persistence()),
        State(repo_with(owner)),
        Path("pending".to_string()),
    )
    .await
    .unwrap();
    let data = response.data.unwrap();
    assert_eq!(data.id, "pending");
    assert_eq!(data.status, RuleStatus::PendingReview);
    assert_eq!(data.updated_at, 88);

    let unauthorized = rule::delete_draft(
        claims(other),
        State(store.clone()),
        State(persistence()),
        Path("pending".to_string()),
    )
    .await
    .unwrap_err();
    assert!(matches!(unauthorized, AppError::Unauthorized(_)));

    let missing = rule::delete_draft(
        claims(owner),
        State(store.clone()),
        State(persistence()),
        Path("missing".to_string()),
    )
    .await
    .unwrap_err();
    assert!(matches!(missing, AppError::NotFound));

    let parse_error = rule::publish_draft(
        claims(owner),
        State(store.clone()),
        State(persistence()),
        State(repo_with(owner)),
        Path("invalid".to_string()),
    )
    .await
    .unwrap_err();
    assert!(matches!(parse_error, AppError::InvalidInput(_)));

    let save_error = rule::save_draft(
        claims(owner),
        State(store.clone()),
        State(persistence()),
        State(repo_with(owner)),
        Json(SaveRuleDraftRequest {
            name: "bad".to_string(),
            player_count: 2,
            description: String::new(),
            design: empty_design(),
            introduction: String::new(),
            cover_url: String::new(),
            screenshot_urls: Vec::new(),
        }),
    )
    .await
    .unwrap_err();
    assert!(matches!(save_error, AppError::InvalidInput(_)));

    let update_error = rule::update_draft(
        claims(owner),
        State(store),
        State(persistence()),
        State(repo_with(owner)),
        Path("pending".to_string()),
        Json(SaveRuleDraftRequest {
            name: "War".to_string(),
            player_count: 2,
            description: String::new(),
            design: valid_design(),
            introduction: String::new(),
            cover_url: "not-a-url".to_string(),
            screenshot_urls: Vec::new(),
        }),
    )
    .await
    .unwrap_err();
    assert!(matches!(update_error, AppError::InvalidInput(_)));
}

#[tokio::test]
async fn fork_and_options_paths_cover_sorting_and_pre_persistence_validation() {
    let owner = Uuid::new_v4();
    let store = store_with(
        Vec::new(),
        vec![
            published_rule("rule-z", "Alpha"),
            published_rule("party", "Party"),
            published_rule("classic", "Classic"),
            published_rule("rule-a", "Beta"),
        ],
    );

    let missing = rule::fork_published_rule(
        claims(owner),
        State(store.clone()),
        State(persistence()),
        State(repo_with(owner)),
        Path("missing".to_string()),
        Json(ForkRuleRequest {
            name: "copy".to_string(),
        }),
    )
    .await
    .unwrap_err();
    assert!(matches!(missing, AppError::NotFound));

    let too_long = rule::fork_published_rule(
        claims(owner),
        State(store.clone()),
        State(persistence()),
        State(repo_with(owner)),
        Path("classic".to_string()),
        Json(ForkRuleRequest {
            name: "x".repeat(300),
        }),
    )
    .await
    .unwrap_err();
    assert!(matches!(too_long, AppError::InvalidInput(_)));

    let Json(response) = rule::rule_options(State(store)).await.unwrap();
    let ids = response
        .data
        .unwrap()
        .into_iter()
        .map(|option| option.id)
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["classic", "party", "rule-a", "rule-z"]);
}

#[tokio::test]
async fn admin_endpoints_stop_on_auth_guard_with_fake_repository() {
    let user_id = Uuid::new_v4();
    let normal_user = User {
        id: UserId(user_id),
        name: "Normal".to_string(),
        email: "normal@example.com".to_string(),
        password: "hashed".to_string(),
        avatar: String::new(),
        role: "user".to_string(),
        banned: false,
    };
    let repo = Arc::new(UserRepository::with_users(vec![normal_user]));
    let store = store_with(
        vec![draft(user_id, "pending", RuleStatus::PendingReview, 1)],
        Vec::new(),
    );

    let list_error =
        rule::list_pending_reviews(claims(user_id), State(store.clone()), State(repo.clone()))
            .await
            .unwrap_err();
    assert!(matches!(list_error, AppError::Forbidden(_)));

    let get_error = rule::admin_get_draft(
        claims(user_id),
        State(store.clone()),
        State(repo.clone()),
        Path("pending".to_string()),
    )
    .await
    .unwrap_err();
    assert!(matches!(get_error, AppError::Forbidden(_)));

    let approve_error = rule::approve_draft(
        claims(user_id),
        State(store.clone()),
        State(persistence()),
        State(repo.clone()),
        Path("pending".to_string()),
    )
    .await
    .unwrap_err();
    assert!(matches!(approve_error, AppError::Forbidden(_)));

    let reject_error = rule::reject_draft(
        claims(user_id),
        State(store),
        State(persistence()),
        State(repo),
        Path("pending".to_string()),
        Json(rule::RejectDraftRequest {
            reason: "not acceptable".to_string(),
        }),
    )
    .await
    .unwrap_err();
    assert!(matches!(reject_error, AppError::Forbidden(_)));
}

fn failing_persistence() -> RulePersistence {
    RulePersistence {
        pool: PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_millis(50))
            .connect_lazy("postgres://user:password@127.0.0.1:1/wildcard_test")
            .unwrap(),
    }
}

#[tokio::test]
async fn persistence_methods_cover_uuid_validation_and_database_error_paths() {
    let persistence = failing_persistence();
    assert!(persistence.ensure_schema().await.is_err());

    let mut repository = RuleRepository::default();
    assert!(persistence.load_into(&mut repository).await.is_err());

    let owner = Uuid::new_v4();
    let draft_id = Uuid::new_v4().to_string();
    let valid_draft = draft(owner, &draft_id, RuleStatus::Draft, 10);

    let mut invalid_owner = valid_draft.clone();
    invalid_owner.owner_id = "not-a-uuid".to_string();
    assert!(matches!(
        persistence.save_draft(&invalid_owner).await.unwrap_err(),
        AppError::InvalidInput(_)
    ));

    let mut invalid_draft_id = valid_draft.clone();
    invalid_draft_id.id = "not-a-uuid".to_string();
    assert!(matches!(
        persistence.save_draft(&invalid_draft_id).await.unwrap_err(),
        AppError::InvalidInput(_)
    ));

    assert!(persistence.save_draft(&valid_draft).await.is_err());

    let mut published = published_rule(&format!("rule_{}", Uuid::new_v4()), "Published");
    published.owner_id = owner.to_string();
    assert!(
        persistence
            .save_published_rule(&published, &draft_id)
            .await
            .is_err()
    );

    let mut invalid_rule = published.clone();
    invalid_rule.id = "not-a-uuid".to_string();
    assert!(matches!(
        persistence
            .save_published_rule(&invalid_rule, &draft_id)
            .await
            .unwrap_err(),
        AppError::InvalidInput(_)
    ));

    assert!(matches!(
        persistence
            .delete_draft("not-a-uuid", &owner.to_string())
            .await
            .unwrap_err(),
        AppError::InvalidInput(_)
    ));
    assert!(
        persistence
            .delete_draft(&draft_id, &owner.to_string())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn admin_review_success_and_conflict_paths_cover_authorized_flow() {
    let admin_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    let admin = User {
        id: UserId(admin_id),
        name: "Admin".to_string(),
        email: "admin@example.com".to_string(),
        password: "hashed".to_string(),
        avatar: String::new(),
        role: "admin".to_string(),
        banned: false,
    };
    let owner = User {
        id: UserId(owner_id),
        name: "Owner".to_string(),
        email: "owner@example.com".to_string(),
        password: "hashed".to_string(),
        avatar: String::new(),
        role: "user".to_string(),
        banned: false,
    };
    let repo = Arc::new(UserRepository::with_users(vec![admin, owner]));
    let mut pending_old = draft(owner_id, "pending-old", RuleStatus::PendingReview, 1);
    pending_old.name = "Old".to_string();
    let mut pending_new = draft(owner_id, "pending-new", RuleStatus::PendingReview, 5);
    pending_new.name = "New".to_string();
    let draft_status = draft(owner_id, "draft-status", RuleStatus::Draft, 10);
    let store = store_with(vec![pending_new, pending_old, draft_status], Vec::new());

    let Json(list_response) =
        rule::list_pending_reviews(claims(admin_id), State(store.clone()), State(repo.clone()))
            .await
            .unwrap();
    let pending = list_response.data.unwrap();
    assert_eq!(
        pending
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Old", "New"]
    );
    assert_eq!(pending[0].owner_name, "Owner");

    let Json(preview) = rule::admin_get_draft(
        claims(admin_id),
        State(store.clone()),
        State(repo.clone()),
        Path("pending-old".to_string()),
    )
    .await
    .unwrap();
    assert_eq!(preview.data.unwrap().id, "pending-old");

    let missing_preview = rule::admin_get_draft(
        claims(admin_id),
        State(store.clone()),
        State(repo.clone()),
        Path("missing".to_string()),
    )
    .await
    .unwrap_err();
    assert!(matches!(missing_preview, AppError::NotFound));

    let approve_conflict = rule::approve_draft(
        claims(admin_id),
        State(store.clone()),
        State(failing_persistence()),
        State(repo.clone()),
        Path("draft-status".to_string()),
    )
    .await
    .unwrap_err();
    assert!(matches!(approve_conflict, AppError::Conflict(_)));

    let reject_conflict = rule::reject_draft(
        claims(admin_id),
        State(store),
        State(failing_persistence()),
        State(repo),
        Path("draft-status".to_string()),
        Json(rule::RejectDraftRequest {
            reason: "valid reason".to_string(),
        }),
    )
    .await
    .unwrap_err();
    assert!(matches!(reject_conflict, AppError::Conflict(_)));
}

fn multipart_body(boundary: &str, content_type: &str, bytes: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"rule.png\"\r\n",
    );
    body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

fn valid_save_payload(name: &str) -> SaveRuleDraftRequest {
    SaveRuleDraftRequest {
        name: name.to_string(),
        player_count: 2,
        description: "desc".to_string(),
        design: valid_design(),
        introduction: "intro".to_string(),
        cover_url: "/static/rule-images/cover.png".to_string(),
        screenshot_urls: vec!["/static/rule-images/shot.png".to_string()],
    }
}

#[tokio::test]
async fn valid_author_write_paths_reach_persistence_error_branches() {
    let owner = Uuid::new_v4();

    let save_error = rule::save_draft(
        claims(owner),
        State(store_with(Vec::new(), Vec::new())),
        State(failing_persistence()),
        State(repo_with(owner)),
        Json(valid_save_payload("Save Valid")),
    )
    .await
    .unwrap_err();
    assert!(matches!(save_error, AppError::DatabaseError(_)));

    let update_id = Uuid::new_v4().to_string();
    let update_store = store_with(
        vec![draft(owner, &update_id, RuleStatus::Draft, 1)],
        Vec::new(),
    );
    let update_error = rule::update_draft(
        claims(owner),
        State(update_store),
        State(failing_persistence()),
        State(repo_with(owner)),
        Path(update_id),
        Json(valid_save_payload("Update Valid")),
    )
    .await
    .unwrap_err();
    assert!(matches!(update_error, AppError::DatabaseError(_)));

    let delete_id = Uuid::new_v4().to_string();
    let mut deleted = draft(owner, &delete_id, RuleStatus::Published, 1);
    deleted.published_rule_id = Some("rule-to-remove".to_string());
    let delete_store = store_with(
        vec![deleted],
        vec![published_rule("rule-to-remove", "Remove")],
    );
    let delete_error = rule::delete_draft(
        claims(owner),
        State(delete_store),
        State(failing_persistence()),
        Path(delete_id),
    )
    .await
    .unwrap_err();
    assert!(matches!(delete_error, AppError::DatabaseError(_)));

    let submit_id = Uuid::new_v4().to_string();
    let submit_store = store_with(
        vec![draft(owner, &submit_id, RuleStatus::Draft, 1)],
        Vec::new(),
    );
    let submit_error = rule::submit_review(
        claims(owner),
        State(submit_store),
        State(failing_persistence()),
        State(repo_with(owner)),
        Path(submit_id),
    )
    .await
    .unwrap_err();
    assert!(matches!(submit_error, AppError::DatabaseError(_)));
}

#[tokio::test]
async fn banned_author_write_paths_stop_before_persistence() {
    let owner = Uuid::new_v4();
    let banned_repo = repo_with_state(owner, "user", true);

    let save_error = rule::save_draft(
        claims(owner),
        State(store_with(Vec::new(), Vec::new())),
        State(failing_persistence()),
        State(banned_repo.clone()),
        Json(valid_save_payload("Save Valid")),
    )
    .await
    .unwrap_err();
    assert!(matches!(save_error, AppError::Forbidden(message) if message == "账号已被封禁，无法执行该操作"));

    let update_id = Uuid::new_v4().to_string();
    let update_error = rule::update_draft(
        claims(owner),
        State(store_with(
            vec![draft(owner, &update_id, RuleStatus::Draft, 1)],
            Vec::new(),
        )),
        State(failing_persistence()),
        State(banned_repo.clone()),
        Path(update_id),
        Json(valid_save_payload("Update Valid")),
    )
    .await
    .unwrap_err();
    assert!(matches!(update_error, AppError::Forbidden(message) if message == "账号已被封禁，无法执行该操作"));

    let submit_id = Uuid::new_v4().to_string();
    let submit_error = rule::submit_review(
        claims(owner),
        State(store_with(
            vec![draft(owner, &submit_id, RuleStatus::Draft, 1)],
            Vec::new(),
        )),
        State(failing_persistence()),
        State(banned_repo.clone()),
        Path(submit_id),
    )
    .await
    .unwrap_err();
    assert!(matches!(submit_error, AppError::Forbidden(message) if message == "账号已被封禁，无法执行该操作"));

    let fork_error = rule::fork_published_rule(
        claims(owner),
        State(store_with(
            Vec::new(),
            vec![published_rule("published-source", "Published Source")],
        )),
        State(failing_persistence()),
        State(banned_repo),
        Path("published-source".to_string()),
        Json(ForkRuleRequest {
            name: "My Copy".to_string(),
        }),
    )
    .await
    .unwrap_err();
    assert!(matches!(fork_error, AppError::Forbidden(message) if message == "账号已被封禁，无法执行该操作"));
}

#[tokio::test]
async fn admin_can_unban_user_through_handler() {
    let admin_id = Uuid::new_v4();
    let target_id = Uuid::new_v4();
    let repo = Arc::new(UserRepository::with_users(vec![
        User {
            id: UserId(admin_id),
            name: "Admin".to_string(),
            email: "admin@example.com".to_string(),
            password: "hashed".to_string(),
            avatar: String::new(),
            role: "admin".to_string(),
            banned: false,
        },
        User {
            id: UserId(target_id),
            name: "Writer".to_string(),
            email: "writer@example.com".to_string(),
            password: "hashed".to_string(),
            avatar: String::new(),
            role: "user".to_string(),
            banned: true,
        },
    ]));

    let Json(response) = rule::unban_user(
        claims(admin_id),
        State(repo.clone()),
        Path(target_id.to_string()),
    )
    .await
    .unwrap();

    assert!(response.success);
    let target = repo.find_by_id(&UserId(target_id)).await.unwrap().unwrap();
    assert!(!target.banned, "解封接口应把 banned 标记清回 false");
}

#[tokio::test]
async fn admin_approve_reject_pending_paths_reach_persistence_error_branches() {
    let admin_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    let admin = User {
        id: UserId(admin_id),
        name: "Admin".to_string(),
        email: "admin@example.com".to_string(),
        password: "hashed".to_string(),
        avatar: String::new(),
        role: "admin".to_string(),
        banned: false,
    };
    let repo = Arc::new(UserRepository::with_users(vec![admin]));

    let approve_id = Uuid::new_v4().to_string();
    let approve_store = store_with(
        vec![draft(owner_id, &approve_id, RuleStatus::PendingReview, 1)],
        Vec::new(),
    );
    let approve_error = rule::approve_draft(
        claims(admin_id),
        State(approve_store),
        State(failing_persistence()),
        State(repo.clone()),
        Path(approve_id),
    )
    .await
    .unwrap_err();
    assert!(matches!(approve_error, AppError::DatabaseError(_)));

    let reject_id = Uuid::new_v4().to_string();
    let reject_store = store_with(
        vec![draft(owner_id, &reject_id, RuleStatus::PendingReview, 1)],
        Vec::new(),
    );
    let reject_error = rule::reject_draft(
        claims(admin_id),
        State(reject_store),
        State(failing_persistence()),
        State(repo),
        Path(reject_id),
        Json(rule::RejectDraftRequest {
            reason: "needs changes".to_string(),
        }),
    )
    .await
    .unwrap_err();
    assert!(matches!(reject_error, AppError::DatabaseError(_)));
}

#[tokio::test]
async fn upload_rule_image_covers_success_and_validation_errors() {
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
        routing::post,
    };
    use tower::ServiceExt;

    let owner = Uuid::new_v4();
    let draft_id = Uuid::new_v4().to_string();
    let store = store_with(
        vec![draft(owner, &draft_id, RuleStatus::Draft, 1)],
        Vec::new(),
    );
    let upload_root = std::env::temp_dir().join(format!("wildcard-rule-upload-{}", Uuid::new_v4()));
    let upload_dir = state::UploadDir::from_path(upload_root.clone());
    let app_state = (store.clone(), upload_dir.clone(), owner, draft_id.clone());
    let app = Router::new()
        .route(
            "/upload",
            post(
                |State((store, upload_dir, owner, draft_id)): State<(
                    RuleStore,
                    state::UploadDir,
                    Uuid,
                    String,
                )>,
                 multipart| async move {
                    rule::upload_rule_image(
                        claims(owner),
                        State(store),
                        State(upload_dir),
                        Path(draft_id),
                        multipart,
                    )
                    .await
                },
            ),
        )
        .with_state(app_state);

    let boundary = "wildcard-rule-boundary";
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/upload")
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(multipart_body(
                    boundary,
                    "image/png",
                    b"rule image",
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let url = json["data"]["imageUrl"].as_str().unwrap();
    assert!(url.starts_with("/static/rule-images/"));
    let saved = upload_root.join(url.trim_start_matches("/static/"));
    assert_eq!(tokio::fs::read(saved).await.unwrap(), b"rule image");

    let bad_boundary = "wildcard-rule-bad-boundary";
    let bad_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/upload")
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={bad_boundary}"),
                )
                .body(Body::from(multipart_body(
                    bad_boundary,
                    "image/gif",
                    b"gif",
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad_response.status(), StatusCode::BAD_REQUEST);
    tokio::fs::remove_dir_all(upload_root).await.unwrap();
}
