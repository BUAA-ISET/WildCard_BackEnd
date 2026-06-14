#![allow(dead_code)]

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
    routing::{get, post},
};
use serde_json::{Value, json};
use tokio::sync::RwLock;
use tower::ServiceExt;
use uuid::Uuid;

mod domain {
    pub mod user {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/domain/user.rs"));
    }
    pub mod report {
        pub use wildcard_backend::domain::report::*;
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
            role: String,
            banned: bool,
            banned_until: Option<i64>,
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
                    banned_until: self.banned_until,
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
                Self::with_user_state(id, name, email, password, avatar, "user", false)
            }

            pub fn with_user_state(
                id: Uuid,
                name: &str,
                email: &str,
                password: &str,
                avatar: &str,
                role: &str,
                banned: bool,
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
                        role: role.to_string(),
                        banned,
                        // banned bool 已迁移到 banned_until 时间戳语义：banned=true 等价于一个远期封禁。
                        banned_until: if banned { Some(i64::MAX) } else { None },
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
                        role: user.role,
                        banned: user.banned,
                        banned_until: None,
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
    test_state_with_repo(
        upload_dir,
        UserRepository::with_user(
            Uuid::from_u128(1),
            "alice",
            "alice@example.com",
            "password123",
            avatar,
        ),
    )
}

fn test_state_with_repo(upload_dir: PathBuf, user_repo: UserRepository) -> TestState {
    TestState {
        jwt_secret: JwtSecret(b"test-secret".to_vec()),
        user: Arc::new(user_repo),
        verification_codes: Arc::new(RwLock::new(HashMap::new())),
        email: EmailSender,
        upload_dir: UploadDir(Arc::new(upload_dir)),
    }
}

fn app(state: TestState) -> Router {
    Router::new()
        .route(
            "/api/user/send-code",
            post(interface::user::send_verification_code),
        )
        .route("/api/user/register", post(interface::user::register))
        .route("/api/user/login", post(interface::user::login))
        .route("/api/user/logout", post(interface::user::logout))
        .route("/api/user/current", get(interface::user::current))
        .route("/api/user/username", post(interface::user::update_username))
        .route("/api/user/password", post(interface::user::update_password))
        .route("/api/user/email", post(interface::user::update_email))
        .route(
            "/api/user/password-reset-code",
            post(interface::user::password_reset_code),
        )
        .route(
            "/api/user/password-reset",
            post(interface::user::password_reset),
        )
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

fn json_request(method: &str, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn login_token(app: Router) -> String {
    let response = app
        .oneshot(json_request(
            "POST",
            "/api/user/login",
            json!({
                "email": "alice",
                "password": "password123"
            }),
        ))
        .await
        .unwrap();
    let body = response_json(response).await;
    body["data"]["token"]
        .as_str()
        .expect("login should produce token")
        .to_string()
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
async fn banned_user_login_returns_forbidden_without_token() {
    let upload_dir = unique_upload_dir("banned-login");
    let app = app(test_state_with_repo(
        upload_dir,
        UserRepository::with_user_state(
            Uuid::from_u128(1),
            "alice",
            "alice@example.com",
            "password123",
            "/static/avatars/existing.png",
            "user",
            true,
        ),
    ));

    let response = app
        .oneshot(json_request(
            "POST",
            "/api/user/login",
            json!({
                "email": "alice",
                "password": "password123"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = response_json(response).await;
    assert_eq!(body["success"], false);
    assert_eq!(body["message"], "账号已被封禁");
    assert!(body["data"].is_null());
}

#[tokio::test]
async fn registration_code_and_password_reset_use_debug_codes_without_smtp() {
    let upload_dir = unique_upload_dir("code-flow");
    let state = test_state(upload_dir, "");
    let app = app(state.clone());

    let send_response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/user/send-code",
            json!({ "email": "NewUser@Example.com" }),
        ))
        .await
        .unwrap();
    assert_eq!(send_response.status(), StatusCode::OK);
    let send_body = response_json(send_response).await;
    let register_code = send_body["debugCode"].as_str().unwrap().to_string();

    let register_response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/user/register",
            json!({
                "email": "newuser@example.com",
                "username": "new-user",
                "password": "secret123",
                "verificationCode": register_code
            }),
        ))
        .await
        .unwrap();
    assert_eq!(register_response.status(), StatusCode::OK);
    let register_body = response_json(register_response).await;
    assert_eq!(register_body["success"], true);
    assert_eq!(register_body["data"]["email"], "newuser@example.com");
    assert!(
        state
            .verification_codes
            .read()
            .await
            .get("newuser@example.com")
            .is_none()
    );

    let reset_code_response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/user/password-reset-code",
            json!({ "email": "alice@example.com" }),
        ))
        .await
        .unwrap();
    assert_eq!(reset_code_response.status(), StatusCode::OK);
    let reset_body = response_json(reset_code_response).await;
    let reset_code = reset_body["debugCode"].as_str().unwrap().to_string();

    let reset_response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/user/password-reset",
            json!({
                "email": "alice@example.com",
                "verificationCode": reset_code,
                "newPassword": "new-password"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(reset_response.status(), StatusCode::OK);
    assert_eq!(response_json(reset_response).await["success"], true);

    let login_response = app
        .oneshot(json_request(
            "POST",
            "/api/user/login",
            json!({
                "email": "alice@example.com",
                "password": "new-password"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(login_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn authenticated_profile_update_paths_cover_success_and_validation() {
    let upload_dir = unique_upload_dir("profile-update");
    let state = test_state(upload_dir, "/static/avatars/existing.png");
    let app = app(state.clone());
    let token = login_token(app.clone()).await;

    let current_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/user/current")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(current_response.status(), StatusCode::OK);
    assert_eq!(
        response_json(current_response).await["data"]["username"],
        "alice"
    );

    let rename_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/username")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "username": "  alice-renamed  " }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rename_response.status(), StatusCode::OK);
    assert_eq!(
        response_json(rename_response).await["data"]["username"],
        "alice-renamed"
    );

    let wrong_password_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/password")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "currentPassword": "wrong", "newPassword": "new-password" })
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong_password_response.status(), StatusCode::BAD_REQUEST);

    let change_password_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/password")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "currentPassword": "password123", "newPassword": "new-password" })
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(change_password_response.status(), StatusCode::OK);

    state.verification_codes.write().await.insert(
        "updated@example.com".to_string(),
        VerificationCodeRecord {
            code: "654321".to_string(),
            expires_at_unix: time::OffsetDateTime::now_utc().unix_timestamp() + 60,
        },
    );
    let email_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/email")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "newEmail": "updated@example.com", "verificationCode": "654321" })
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(email_response.status(), StatusCode::OK);
    assert_eq!(
        response_json(email_response).await["data"]["email"],
        "updated@example.com"
    );
}

#[tokio::test]
async fn user_validation_paths_return_errors_before_mutating_state() {
    let upload_dir = unique_upload_dir("validation");
    let app = app(test_state(upload_dir, ""));

    for (uri, body) in [
        ("/api/user/send-code", json!({ "email": "not-an-email" })),
        (
            "/api/user/register",
            json!({
                "email": "valid@example.com",
                "username": " ",
                "password": "secret",
                "verificationCode": "123456"
            }),
        ),
        (
            "/api/user/login",
            json!({ "email": " ", "password": "secret" }),
        ),
        (
            "/api/user/password-reset",
            json!({
                "email": "alice@example.com",
                "verificationCode": " ",
                "newPassword": "secret"
            }),
        ),
    ] {
        let response = app
            .clone()
            .oneshot(json_request("POST", uri, body))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response_json(response).await["success"], false);
    }

    let logout_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/logout")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logout_response.status(), StatusCode::OK);
    assert_eq!(response_json(logout_response).await["success"], true);
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
