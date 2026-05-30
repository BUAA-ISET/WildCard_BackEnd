#![allow(dead_code)]

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
    routing::post,
};
use serde_json::{Value, json};
use tokio::sync::RwLock;
use tower::ServiceExt;
use uuid::Uuid;

mod domain {
    pub mod user {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/domain/user.rs"));
    }
}

mod error {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/error.rs"));
}

mod infrastructure {
    pub mod email {
        #[derive(Clone, Debug, Default)]
        pub struct EmailSender;

        impl EmailSender {
            pub fn is_configured(&self) -> bool {
                false
            }

            pub async fn send_verification_code(
                &self,
                _email: &str,
                _code: &str,
            ) -> Result<(), String> {
                Ok(())
            }
        }
    }

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
        }

        impl StoredUser {
            fn into_user(self) -> User {
                User {
                    id: UserId(self.id),
                    name: self.name,
                    email: self.email,
                    password: self.password,
                    avatar: self.avatar,
                    role: "user".to_string(),
                }
            }
        }

        #[derive(Debug, Default)]
        pub struct UserRepository {
            users: Arc<RwLock<HashMap<Uuid, StoredUser>>>,
        }

        impl UserRepository {
            pub fn with_user(
                id: Uuid,
                name: &str,
                email: &str,
                password: &str,
                avatar: &str,
            ) -> Self {
                let mut users = HashMap::new();
                users.insert(
                    id,
                    StoredUser {
                        id,
                        name: name.to_string(),
                        email: email.to_string(),
                        password: password.to_string(),
                        avatar: avatar.to_string(),
                    },
                );

                Self {
                    users: Arc::new(RwLock::new(users)),
                }
            }

            pub async fn register(&self, user: User) -> Result<(), AppError> {
                let mut users = self.users.write().await;
                if users.values().any(|stored| stored.name == user.name) {
                    return Err(AppError::UserAlreadyExist("用户名已存在".to_string()));
                }
                if users.values().any(|stored| stored.email == user.email) {
                    return Err(AppError::UserAlreadyExist("该邮箱已注册".to_string()));
                }

                users.insert(
                    user.id.0,
                    StoredUser {
                        id: user.id.0,
                        name: user.name,
                        email: user.email,
                        password: user.password,
                        avatar: user.avatar,
                    },
                );
                Ok(())
            }

            pub async fn find_by_name(&self, name: &str) -> Result<Option<User>, AppError> {
                Ok(self
                    .users
                    .read()
                    .await
                    .values()
                    .find(|user| user.name == name)
                    .cloned()
                    .map(StoredUser::into_user))
            }

            pub async fn find_by_email(&self, email: &str) -> Result<Option<User>, AppError> {
                Ok(self
                    .users
                    .read()
                    .await
                    .values()
                    .find(|user| user.email == email)
                    .cloned()
                    .map(StoredUser::into_user))
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

            pub async fn update_username(
                &self,
                user_id: &UserId,
                username: &str,
            ) -> Result<User, AppError> {
                let mut users = self.users.write().await;
                let user = users
                    .get_mut(&user_id.0)
                    .ok_or_else(|| AppError::Unauthorized("未登录".to_string()))?;
                user.name = username.to_string();
                Ok(user.clone().into_user())
            }

            pub async fn update_email(
                &self,
                user_id: &UserId,
                email: &str,
            ) -> Result<User, AppError> {
                let mut users = self.users.write().await;
                let user = users
                    .get_mut(&user_id.0)
                    .ok_or_else(|| AppError::Unauthorized("未登录".to_string()))?;
                user.email = email.to_string();
                Ok(user.clone().into_user())
            }

            pub async fn update_password(
                &self,
                user_id: &UserId,
                new_password: &str,
            ) -> Result<(), AppError> {
                let mut users = self.users.write().await;
                let user = users
                    .get_mut(&user_id.0)
                    .ok_or_else(|| AppError::Unauthorized("未登录".to_string()))?;
                user.password = new_password.to_string();
                Ok(())
            }

            pub async fn update_avatar(
                &self,
                user_id: &UserId,
                avatar: &str,
            ) -> Result<User, AppError> {
                let mut users = self.users.write().await;
                let user = users
                    .get_mut(&user_id.0)
                    .ok_or_else(|| AppError::Unauthorized("未登录".to_string()))?;
                user.avatar = avatar.to_string();
                Ok(user.clone().into_user())
            }

            pub fn check_password(password: &str, stored_password: &str) -> bool {
                password == stored_password
            }
        }
    }
}

mod state {
    use std::{collections::HashMap, path::PathBuf, sync::Arc};

    use axum::extract::FromRef;
    use tokio::sync::RwLock;

    use crate::{
        TestState, infrastructure::email::EmailSender, infrastructure::user::UserRepository,
    };

    #[derive(Clone, Debug)]
    pub struct JwtSecret(pub Vec<u8>);

    #[derive(Clone, Debug)]
    pub struct UploadDir(pub Arc<PathBuf>);

    impl std::ops::Deref for UploadDir {
        type Target = PathBuf;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    #[derive(Clone, Debug)]
    pub struct VerificationCodeRecord {
        pub code: String,
        pub expires_at_unix: i64,
    }

    impl FromRef<TestState> for JwtSecret {
        fn from_ref(input: &TestState) -> Self {
            input.jwt_secret.clone()
        }
    }

    impl FromRef<TestState> for Arc<UserRepository> {
        fn from_ref(input: &TestState) -> Self {
            input.user.clone()
        }
    }

    impl FromRef<TestState> for Arc<RwLock<HashMap<String, VerificationCodeRecord>>> {
        fn from_ref(input: &TestState) -> Self {
            input.verification_codes.clone()
        }
    }

    impl FromRef<TestState> for EmailSender {
        fn from_ref(input: &TestState) -> Self {
            input.email.clone()
        }
    }

    impl FromRef<TestState> for UploadDir {
        fn from_ref(input: &TestState) -> Self {
            input.upload_dir.clone()
        }
    }
}

mod interface {
    pub mod auth {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/interface/auth.rs"
        ));
    }

    pub mod user {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/interface/user.rs"
        ));
    }
}

use infrastructure::{email::EmailSender, user::UserRepository};
use state::{JwtSecret, UploadDir, VerificationCodeRecord};

#[derive(Clone, Debug)]
struct TestState {
    jwt_secret: JwtSecret,
    user: Arc<UserRepository>,
    verification_codes: Arc<RwLock<HashMap<String, VerificationCodeRecord>>>,
    email: EmailSender,
    upload_dir: UploadDir,
}

fn test_state(upload_dir: PathBuf, avatar: &str) -> TestState {
    TestState {
        jwt_secret: JwtSecret(b"test-secret".to_vec()),
        user: Arc::new(UserRepository::with_user(
            Uuid::from_u128(1),
            "alice",
            "alice@example.com",
            "password123",
            avatar,
        )),
        verification_codes: Arc::new(RwLock::new(HashMap::new())),
        email: EmailSender,
        upload_dir: UploadDir(Arc::new(upload_dir)),
    }
}

fn app(state: TestState) -> Router {
    Router::new()
        .route("/api/user/login", post(interface::user::login))
        .route("/api/user/avatar", post(interface::user::update_avatar))
        .with_state(state)
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&bytes).expect("response body should be JSON")
}

fn unique_upload_dir(test_name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("wildcard-backend-{test_name}-{}", Uuid::new_v4()))
}

fn multipart_body(boundary: &str, content_type: &str, bytes: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"avatar\"; filename=\"avatar.png\"\r\n",
    );
    body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

#[tokio::test]
async fn login_accepts_username_in_email_field() {
    let upload_dir = unique_upload_dir("login");
    let app = app(test_state(upload_dir, "/static/avatars/existing.png"));

    let request = Request::builder()
        .method("POST")
        .uri("/api/user/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "email": "alice",
                "password": "password123"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response_json(response).await;
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["username"], "alice");
    assert_eq!(body["data"]["email"], "alice@example.com");
    assert_eq!(body["data"]["avatar"], "/static/avatars/existing.png");
    assert!(
        body["data"]["token"]
            .as_str()
            .is_some_and(|token| !token.is_empty())
    );
}

#[tokio::test]
async fn avatar_upload_updates_profile_url_and_removes_previous_file() {
    let upload_dir = unique_upload_dir("avatar");
    let avatars_dir = upload_dir.join("avatars");
    tokio::fs::create_dir_all(&avatars_dir).await.unwrap();
    let old_avatar_path = avatars_dir.join("old.png");
    tokio::fs::write(&old_avatar_path, b"old image")
        .await
        .unwrap();

    let app = app(test_state(upload_dir.clone(), "/static/avatars/old.png"));

    let login_request = Request::builder()
        .method("POST")
        .uri("/api/user/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "email": "alice",
                "password": "password123"
            })
            .to_string(),
        ))
        .unwrap();
    let login_response = app.clone().oneshot(login_request).await.unwrap();
    let login_body = response_json(login_response).await;
    let token = login_body["data"]["token"]
        .as_str()
        .expect("login should return a bearer token");

    let boundary = "wildcard-avatar-boundary";
    let uploaded_png = b"\x89PNG\r\n\x1a\nnew avatar bytes";
    let avatar_request = Request::builder()
        .method("POST")
        .uri("/api/user/avatar")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from(multipart_body(
            boundary,
            "image/png",
            uploaded_png,
        )))
        .unwrap();

    let avatar_response = app.oneshot(avatar_request).await.unwrap();
    assert_eq!(avatar_response.status(), StatusCode::OK);

    let avatar_body = response_json(avatar_response).await;
    let avatar_url = avatar_body["data"]["avatar"]
        .as_str()
        .expect("avatar upload should return the updated avatar URL");

    assert_eq!(avatar_body["success"], true);
    assert_eq!(avatar_body["data"]["username"], "alice");
    assert!(avatar_url.starts_with("/static/avatars/"));
    assert!(avatar_url.ends_with(".png"));
    assert!(!tokio::fs::try_exists(&old_avatar_path).await.unwrap());

    let uploaded_name = avatar_url.trim_start_matches("/static/avatars/");
    let uploaded_path = avatars_dir.join(uploaded_name);
    assert_eq!(tokio::fs::read(uploaded_path).await.unwrap(), uploaded_png);
    tokio::fs::remove_dir_all(&upload_dir).await.unwrap();
}
