#![allow(dead_code)]

mod error {
    pub use wildcard_backend::error::*;
}

mod domain {
    pub mod room {
        pub use wildcard_backend::domain::room::*;
    }
    pub mod rule_engine {
        pub use wildcard_backend::domain::rule_engine::*;
    }
    pub mod user {
        pub use wildcard_backend::domain::user::*;
    }
}

mod infrastructure {
    pub mod user {
        use crate::{
            domain::user::{User, UserId},
            error::AppError,
        };
        use sqlx::PgPool;

        #[derive(Debug)]
        pub struct UserRepository {
            pub pool: PgPool,
        }

        impl UserRepository {
            pub async fn find_by_id(&self, user_id: &UserId) -> Result<Option<User>, AppError> {
                // 默认返回一个未封禁的普通用户，让 create_review 的 ban 校验通过，
                // 以便测试覆盖到后续的评分 / 规则存在性 / 图片长度分支。
                Ok(Some(User {
                    id: UserId(user_id.0),
                    name: "market-user".to_string(),
                    email: "market@example.com".to_string(),
                    password: "hashed".to_string(),
                    avatar: String::new(),
                    role: "user".to_string(),
                    banned: false,
                }))
            }
        }
    }
}

mod interface {
    pub mod auth {
        use crate::domain::user::UserId;
        use serde::{Deserialize, Serialize};

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
        use crate::domain::rule_engine::{ExportedRuleDesign, RuntimeRule};
        use serde::Serialize;
        use sqlx::PgPool;
        use std::collections::HashMap;

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

        #[derive(Debug, Clone)]
        pub struct RulePersistence {
            pub pool: PgPool,
        }

        #[derive(Debug, Clone)]
        pub struct PublishedRule {
            pub id: String,
            pub owner_id: String,
            pub name: String,
            pub player_count: u8,
            pub description: String,
            pub version: u32,
            pub design: ExportedRuleDesign,
            pub runtime: RuntimeRule,
            pub created_at: i64,
            pub updated_at: i64,
            pub introduction: String,
            pub cover_url: String,
            pub screenshot_urls: Vec<String>,
            pub banned: bool,
        }

        #[derive(Debug, Default)]
        pub struct RuleRepository {
            pub published: HashMap<String, PublishedRule>,
        }

        pub async fn ensure_not_banned(
            user_id: &crate::domain::user::UserId,
            user_repo: &std::sync::Arc<crate::infrastructure::user::UserRepository>,
        ) -> Result<(), crate::error::AppError> {
            let user = user_repo.find_by_id(user_id).await?.ok_or(
                crate::error::AppError::Unauthorized("用户不存在".to_string()),
            )?;
            if user.banned {
                return Err(crate::error::AppError::Forbidden(
                    "账号已被封禁，无法执行该操作".to_string(),
                ));
            }
            Ok(())
        }
    }

    pub mod room {
        use crate::{
            domain::{room::Room, rule_engine::GameSession},
            state::RoomStore,
        };
        use std::{collections::HashMap, sync::Arc};
        use tokio::sync::RwLock;

        #[derive(Debug, Default)]
        pub struct RoomRepository {
            pub rooms: HashMap<String, Room>,
            pub player_rooms: HashMap<String, String>,
            pub sessions: HashMap<String, GameSession>,
        }

        pub fn build_room_store() -> RoomStore {
            Arc::new(RwLock::new(RoomRepository::default()))
        }
    }

    pub mod market {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/interface/market.rs"
        ));

        #[cfg(test)]
        mod endpoint_coverage {
            use super::*;
            use axum::{
                Router,
                body::{Body, to_bytes},
                http::{Request, StatusCode, header},
                routing::post,
            };
            use sqlx::postgres::PgPoolOptions;
            use tokio::sync::RwLock;
            use tower::ServiceExt;
            use uuid::Uuid;

            use crate::{
                domain::{
                    rule_engine::{ExportedRuleDesign, RuleEngine},
                    user::UserId,
                },
                infrastructure::user::UserRepository,
                interface::{
                    auth::TokenClaims,
                    rule::{PublishedRule, RulePersistence, RuleRepository},
                },
                state::{RuleStore, UploadDir},
            };

            fn lazy_pool() -> sqlx::PgPool {
                PgPoolOptions::new()
                    .acquire_timeout(std::time::Duration::from_millis(50))
                    .connect_lazy("postgres://user:password@127.0.0.1:1/wildcard_test")
                    .unwrap()
            }

            fn claims() -> TokenClaims {
                TokenClaims {
                    user_id: UserId(Uuid::new_v4()),
                    iat: 0,
                    exp: usize::MAX,
                }
            }

            fn valid_design() -> ExportedRuleDesign {
                let content = std::fs::read_to_string(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("war.json"),
                )
                .unwrap();
                serde_json::from_str(&content).unwrap()
            }

            fn published_rule(
                id: &str,
                owner_id: &str,
                name: &str,
                created_at: i64,
            ) -> PublishedRule {
                let design = valid_design();
                let runtime =
                    RuleEngine::parse(name.to_string(), 2, "desc".to_string(), design.clone())
                        .unwrap();
                PublishedRule {
                    id: id.to_string(),
                    owner_id: owner_id.to_string(),
                    name: name.to_string(),
                    player_count: 2,
                    description: format!("{name} description"),
                    version: 1,
                    design,
                    runtime,
                    created_at,
                    updated_at: created_at,
                    introduction: format!("{name} intro"),
                    cover_url: format!("/static/rule-images/{id}.png"),
                    screenshot_urls: vec![format!("/static/rule-images/{id}-1.png")],
                    banned: false,
                }
            }

            fn store_with(rules: Vec<PublishedRule>) -> RuleStore {
                Arc::new(RwLock::new(RuleRepository {
                    published: rules
                        .into_iter()
                        .map(|rule| (rule.id.clone(), rule))
                        .collect(),
                }))
            }

            fn multipart_body(boundary: &str, content_type: &str, bytes: &[u8]) -> Vec<u8> {
                let mut body = Vec::new();
                body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
                body.extend_from_slice(
                    b"Content-Disposition: form-data; name=\"file\"; filename=\"review.png\"\r\n",
                );
                body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
                body.extend_from_slice(bytes);
                body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
                body
            }

            #[tokio::test]
            async fn list_detail_and_developer_paths_cover_filters_and_db_fallbacks() {
                let pool = lazy_pool();
                let user_repo = Arc::new(UserRepository { pool: pool.clone() });
                let persistence = RulePersistence { pool };
                let store = store_with(vec![
                    published_rule("builtin-alpha", "builtin-owner", "Alpha", 10),
                    published_rule("builtin-beta", "builtin-owner", "Beta", 30),
                    published_rule("builtin-other", "other-owner", "Other", 20),
                ]);

                let Json(all) = list_published_rules(
                    State(store.clone()),
                    State(user_repo.clone()),
                    State(persistence.clone()),
                    Query(RuleQueryParams {
                        keyword: None,
                        rule_type: None,
                    }),
                )
                .await
                .unwrap();
                let all = all.data.unwrap();
                assert_eq!(
                    all.iter().map(|rule| rule.id.as_str()).collect::<Vec<_>>(),
                    vec!["builtin-beta", "builtin-other", "builtin-alpha"]
                );
                assert_eq!(all[0].developer.name, "WildCard 内置");
                assert_eq!(all[0].rating, 0.0);

                let Json(filtered) = list_published_rules(
                    State(store.clone()),
                    State(user_repo.clone()),
                    State(persistence.clone()),
                    Query(RuleQueryParams {
                        keyword: Some("alp".to_string()),
                        rule_type: Some(DEFAULT_RULE_TYPE.to_string()),
                    }),
                )
                .await
                .unwrap();
                assert_eq!(filtered.data.unwrap()[0].id, "builtin-alpha");

                let Json(detail) = get_published_rule_detail(
                    State(store.clone()),
                    State(user_repo.clone()),
                    State(persistence.clone()),
                    Path("builtin-alpha".to_string()),
                )
                .await
                .unwrap();
                let detail = detail.data.unwrap();
                assert_eq!(detail.summary.id, "builtin-alpha");
                assert_eq!(detail.introduction, "Alpha intro");
                assert_eq!(detail.screenshots.len(), 1);
                assert!(detail.reviews.is_empty());

                let Json(developer_rules) = list_developer_rules(
                    State(store),
                    State(user_repo),
                    State(persistence),
                    Path("builtin-owner".to_string()),
                    Query(RuleQueryParams {
                        keyword: Some("desc".to_string()),
                        rule_type: None,
                    }),
                )
                .await
                .unwrap();
                assert_eq!(developer_rules.data.unwrap().len(), 2);
            }

            #[tokio::test]
            async fn review_validation_and_upload_image_paths_cover_new_market_features() {
                let pool = lazy_pool();
                let user_repo = Arc::new(UserRepository { pool: pool.clone() });
                let persistence = RulePersistence { pool };
                let store = store_with(vec![published_rule(
                    "builtin-alpha",
                    "builtin-owner",
                    "Alpha",
                    10,
                )]);

                let invalid_rating = create_review(
                    claims(),
                    State(user_repo.clone()),
                    State(persistence.clone()),
                    State(store.clone()),
                    Path("builtin-alpha".to_string()),
                    Json(CreateReviewRequest {
                        rating: 6,
                        content: "bad".to_string(),
                        image_url: None,
                    }),
                )
                .await
                .unwrap_err();
                assert!(matches!(invalid_rating, AppError::InvalidInput(_)));

                let missing_rule = create_review(
                    claims(),
                    State(user_repo.clone()),
                    State(persistence.clone()),
                    State(store.clone()),
                    Path("missing".to_string()),
                    Json(CreateReviewRequest {
                        rating: 5,
                        content: "ok".to_string(),
                        image_url: None,
                    }),
                )
                .await
                .unwrap_err();
                assert!(matches!(missing_rule, AppError::NotFound));

                let long_image = create_review(
                    claims(),
                    State(user_repo),
                    State(persistence),
                    State(store),
                    Path("builtin-alpha".to_string()),
                    Json(CreateReviewRequest {
                        rating: 5,
                        content: "ok".to_string(),
                        image_url: Some(format!("/{}", "x".repeat(600))),
                    }),
                )
                .await
                .unwrap_err();
                assert!(
                    matches!(long_image, AppError::InvalidInput(message) if message.contains("图片地址"))
                );

                let upload_root =
                    std::env::temp_dir().join(format!("wildcard-review-upload-{}", Uuid::new_v4()));
                let upload_dir = UploadDir::from_path(upload_root.clone());
                let app = Router::new()
                    .route(
                        "/upload",
                        post(
                            |State(upload_dir): State<UploadDir>, multipart| async move {
                                upload_review_image(claims(), State(upload_dir), multipart).await
                            },
                        ),
                    )
                    .with_state(upload_dir);

                let boundary = "wildcard-review-boundary";
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
                                b"png bytes",
                            )))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::OK);
                let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
                let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
                let url = json["data"]["imageUrl"].as_str().unwrap();
                assert!(url.starts_with("/static/review-images/"));
                let saved = upload_root.join(url.trim_start_matches("/static/"));
                assert_eq!(tokio::fs::read(saved).await.unwrap(), b"png bytes");

                let bad_boundary = "wildcard-review-bad-boundary";
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
        }
    }
}

mod state {
    use std::{path::PathBuf, sync::Arc};
    use tokio::sync::RwLock;

    #[derive(Clone, Debug)]
    pub struct UploadDir(pub Arc<PathBuf>);

    impl UploadDir {
        pub fn from_path(path: PathBuf) -> Self {
            Self(Arc::new(path))
        }
    }

    pub type RoomStore = Arc<RwLock<crate::interface::room::RoomRepository>>;
    pub type RuleStore = Arc<RwLock<crate::interface::rule::RuleRepository>>;
}
