use std::sync::Arc;

use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, header},
};
use axum_extra::extract::cookie::{Cookie, SameSite};
use jsonwebtoken::{EncodingKey, Header};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use super::auth::TokenClaims;
use crate::error::AppError;
use crate::infrastructure::user::UserRepository;
use crate::{
    domain::user::{User, UserId},
    state::{JwtSecret, VerificationCodeRecord},
};

#[derive(Debug, Deserialize)]
pub struct RegisterUserRequest {
    pub email: String,
    pub username: String,
    pub password: String,
    #[serde(rename = "verificationCode")]
    pub verification_code: String,
}

#[derive(Debug, Deserialize)]
pub struct FindUserRequest {
    pub user_name: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct UserDto {
    pub id: UserId,
    pub username: String,
    pub email: String,
    pub avatar: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

impl UserDto {
    fn from_user(user: User) -> Self {
        Self {
            id: user.id,
            username: user.name,
            email: user.email,
            avatar: String::new(),
            token: None,
        }
    }

    fn from_user_with_token(user: User, token: String) -> Self {
        let mut dto = Self::from_user(user);
        dto.token = Some(token);
        dto
    }
}

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(rename = "debugCode", skip_serializing_if = "Option::is_none")]
    pub debug_code: Option<String>,
}

impl<T> ApiResponse<T> {
    pub(crate) fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: None,
            debug_code: None,
        }
    }

    pub(crate) fn success_with_optional_data(data: Option<T>) -> Self {
        Self {
            success: true,
            data,
            message: None,
            debug_code: None,
        }
    }

    pub(crate) fn failure(message: String) -> Self {
        Self {
            success: false,
            data: None,
            message: Some(message),
            debug_code: None,
        }
    }
}

impl ApiResponse<()> {
    pub(crate) fn success_without_data(message: Option<String>) -> Self {
        Self {
            success: true,
            data: None,
            message,
            debug_code: None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SendVerificationCodeRequest {
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUsernameRequest {
    pub username: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePasswordRequest {
    #[serde(rename = "currentPassword")]
    pub current_password: String,
    #[serde(rename = "newPassword")]
    pub new_password: String,
}

fn validate_email(email: &str) -> Result<String, AppError> {
    let normalized = email.trim().to_lowercase();
    if normalized.is_empty() {
        return Err(AppError::InvalidInput("请输入邮箱".to_string()));
    }
    if !normalized.contains('@') {
        return Err(AppError::InvalidInput("邮箱格式不正确".to_string()));
    }
    Ok(normalized)
}

fn validate_username(username: &str) -> Result<String, AppError> {
    let normalized = username.trim().to_string();
    if normalized.is_empty() {
        return Err(AppError::InvalidInput("用户名不能为空".to_string()));
    }
    Ok(normalized)
}

fn validate_password(password: &str, empty_message: &str) -> Result<String, AppError> {
    let normalized = password.trim().to_string();
    if normalized.is_empty() {
        return Err(AppError::InvalidInput(empty_message.to_string()));
    }
    Ok(normalized)
}

fn generate_code() -> String {
    let value = time::OffsetDateTime::now_utc()
        .unix_timestamp_nanos()
        .unsigned_abs()
        % 900_000;
    format!("{:06}", value + 100_000)
}

fn build_token(user_id: UserId, jwt_secret: &[u8]) -> Result<String, AppError> {
    let now = time::OffsetDateTime::now_utc();
    let exp = now + time::Duration::days(1);

    let claims = TokenClaims {
        user_id,
        exp: exp.unix_timestamp() as usize,
        iat: now.unix_timestamp() as usize,
    };

    jsonwebtoken::encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret),
    )
    .map_err(|e| AppError::InvalidInput(format!("Token 生成失败: {e}")))
}

fn build_auth_cookie(token: &str) -> Cookie<'static> {
    Cookie::build(("token", token.to_string()))
        .path("/")
        .max_age(time::Duration::days(1))
        .same_site(SameSite::Lax)
        .http_only(true)
        .build()
}

fn clear_auth_cookie() -> Cookie<'static> {
    Cookie::build(("token", String::new()))
        .path("/")
        .max_age(time::Duration::days(-1))
        .same_site(SameSite::Lax)
        .http_only(true)
        .build()
}

#[tracing::instrument]
pub async fn find(
    State(user_repo): State<Arc<UserRepository>>,
    Query(payload): Query<FindUserRequest>,
) -> Result<Json<ApiResponse<UserDto>>, AppError> {
    user_repo
        .find_by_name(&payload.user_name)
        .await?
        .map(UserDto::from_user)
        .map(ApiResponse::success)
        .map(Json)
        .ok_or(AppError::NotFound)
}

#[tracing::instrument]
pub async fn send_verification_code(
    State(user_repo): State<Arc<UserRepository>>,
    State(codes): State<Arc<RwLock<std::collections::HashMap<String, VerificationCodeRecord>>>>,
    Json(payload): Json<SendVerificationCodeRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let email = validate_email(&payload.email)?;

    if user_repo.find_by_email(&email).await?.is_some() {
        return Err(AppError::InvalidInput("该邮箱已注册".to_string()));
    }

    let code = generate_code();
    let expires_at_unix =
        (time::OffsetDateTime::now_utc() + time::Duration::minutes(5)).unix_timestamp();
    let mut guard = codes.write().await;
    guard.insert(
        email,
        VerificationCodeRecord {
            code: code.clone(),
            expires_at_unix,
        },
    );

    Ok(Json(ApiResponse {
        success: true,
        data: None,
        message: Some("验证码已发送，请检查邮箱".to_string()),
        debug_code: Some(code),
    }))
}

#[tracing::instrument]
pub async fn register(
    State(JwtSecret(jwt_secret)): State<JwtSecret>,
    State(user_repo): State<Arc<UserRepository>>,
    State(codes): State<Arc<RwLock<std::collections::HashMap<String, VerificationCodeRecord>>>>,
    Json(payload): Json<RegisterUserRequest>,
) -> Result<(HeaderMap, Json<ApiResponse<UserDto>>), AppError> {
    let email = validate_email(&payload.email)?;
    let username = validate_username(&payload.username)?;
    let password = validate_password(&payload.password, "请输入密码")?;
    let verification_code = payload.verification_code.trim().to_string();

    if verification_code.is_empty() {
        return Err(AppError::InvalidInput("请先发送验证码".to_string()));
    }

    {
        let mut guard = codes.write().await;
        let stored = guard
            .get(&email)
            .cloned()
            .ok_or_else(|| AppError::InvalidInput("请先发送验证码".to_string()))?;

        if time::OffsetDateTime::now_utc().unix_timestamp() > stored.expires_at_unix {
            guard.remove(&email);
            return Err(AppError::InvalidInput(
                "验证码已过期，请重新发送".to_string(),
            ));
        }

        if stored.code != verification_code {
            return Err(AppError::InvalidInput("验证码错误".to_string()));
        }

        guard.remove(&email);
    }

    let user = User {
        id: UserId(Uuid::new_v4()),
        name: username,
        email: email.clone(),
        password,
    };
    user_repo.register(user).await?;

    let created = user_repo
        .find_by_email(&email)
        .await?
        .ok_or(AppError::NotFound)?;

    let token = build_token(created.id.clone(), &jwt_secret)?;
    let cookie = build_auth_cookie(&token);
    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, cookie.to_string().parse().unwrap());

    Ok((
        headers,
        Json(ApiResponse::success(UserDto::from_user_with_token(
            created, token,
        ))),
    ))
}

#[tracing::instrument]
pub async fn login(
    State(JwtSecret(jwt_secret)): State<JwtSecret>,
    State(user_repo): State<Arc<UserRepository>>,
    Json(payload): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<ApiResponse<UserDto>>), AppError> {
    let email = validate_email(&payload.email)?;
    let password = validate_password(&payload.password, "请输入邮箱和密码")?;
    let user = user_repo.find_by_email(&email).await?;

    if let Some(user) = user {
        let is_valid = UserRepository::check_password(&password, &user.password);
        if !is_valid {
            return Err(AppError::InvalidInput("邮箱或密码错误".to_string()));
        }

        let token = build_token(user.id.clone(), &jwt_secret)?;
        let cookie = build_auth_cookie(&token);

        let mut headers = HeaderMap::new();
        headers.insert(header::SET_COOKIE, cookie.to_string().parse().unwrap());

        Ok((
            headers,
            Json(ApiResponse::success(UserDto::from_user_with_token(
                user, token,
            ))),
        ))
    } else {
        Err(AppError::InvalidInput("邮箱或密码错误".to_string()))
    }
}

#[tracing::instrument]
pub async fn logout() -> (HeaderMap, Json<ApiResponse<()>>) {
    let cookie = clear_auth_cookie();
    let mut header = HeaderMap::new();
    header.insert(header::SET_COOKIE, cookie.to_string().parse().unwrap());
    (
        header,
        Json(ApiResponse::success_without_data(Some(
            "已退出登录".to_string(),
        ))),
    )
}

#[tracing::instrument]
pub async fn current(
    TokenClaims { user_id, .. }: TokenClaims,
    State(user_repo): State<Arc<UserRepository>>,
) -> Result<Json<ApiResponse<UserDto>>, AppError> {
    user_repo
        .find_by_id(&user_id)
        .await?
        .map(UserDto::from_user)
        .map(ApiResponse::success)
        .map(Json)
        .ok_or(AppError::Unauthorized("未登录".to_string()))
}

#[tracing::instrument]
pub async fn update_username(
    TokenClaims { user_id, .. }: TokenClaims,
    State(user_repo): State<Arc<UserRepository>>,
    Json(payload): Json<UpdateUsernameRequest>,
) -> Result<Json<ApiResponse<UserDto>>, AppError> {
    let username = validate_username(&payload.username)?;
    let user = user_repo.update_username(&user_id, &username).await?;

    Ok(Json(ApiResponse::success(UserDto::from_user(user))))
}

#[tracing::instrument]
pub async fn update_password(
    TokenClaims { user_id, .. }: TokenClaims,
    State(user_repo): State<Arc<UserRepository>>,
    Json(payload): Json<UpdatePasswordRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let current_password = validate_password(&payload.current_password, "请填写所有密码字段")?;
    let new_password = validate_password(&payload.new_password, "请填写所有密码字段")?;

    let user = user_repo
        .find_by_id(&user_id)
        .await?
        .ok_or(AppError::Unauthorized("未登录".to_string()))?;

    if !UserRepository::check_password(&current_password, &user.password) {
        return Err(AppError::InvalidInput("当前密码错误".to_string()));
    }

    user_repo.update_password(&user_id, &new_password).await?;

    Ok(Json(ApiResponse::success_without_data(Some(
        "密码更新成功".to_string(),
    ))))
}
