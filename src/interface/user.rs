use std::sync::Arc;

use axum::{
    Json,
    extract::{Multipart, Query, State},
    http::{HeaderMap, header},
};
use axum_extra::extract::cookie::{Cookie, SameSite};
use jsonwebtoken::{EncodingKey, Header};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use super::auth::TokenClaims;
use crate::error::AppError;
use crate::infrastructure::email::EmailSender;
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
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

impl UserDto {
    fn from_user(user: User) -> Self {
        Self {
            id: user.id,
            username: user.name,
            email: user.email,
            avatar: user.avatar,
            role: user.role,
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

    #[allow(dead_code)]
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

#[derive(Debug, Deserialize)]
pub struct UpdateEmailRequest {
    #[serde(rename = "newEmail")]
    pub new_email: String,
    #[serde(rename = "verificationCode")]
    pub verification_code: String,
}

#[derive(Debug, Deserialize)]
pub struct PasswordResetCodeRequest {
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct PasswordResetRequest {
    pub email: String,
    #[serde(rename = "verificationCode")]
    pub verification_code: String,
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

fn validate_login_account(account: &str) -> Result<String, AppError> {
    let normalized = account.trim().to_string();
    if normalized.is_empty() {
        return Err(AppError::InvalidInput("请输入邮箱或用户名".to_string()));
    }
    Ok(normalized)
}

fn looks_like_email(account: &str) -> bool {
    account.contains('@')
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
    use rand::Rng;
    let value: u32 = rand::rng().random_range(0..1_000_000);
    format!("{value:06}")
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
    .map_err(|e| AppError::InvalidInput(format!("Token 生成失败：{e}")))
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

#[tracing::instrument(skip(email_sender))]
pub async fn send_verification_code(
    State(user_repo): State<Arc<UserRepository>>,
    State(codes): State<Arc<RwLock<std::collections::HashMap<String, VerificationCodeRecord>>>>,
    State(email_sender): State<EmailSender>,
    Json(payload): Json<SendVerificationCodeRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let email = validate_email(&payload.email)?;

    if user_repo.find_by_email(&email).await?.is_some() {
        return Err(AppError::InvalidInput("该邮箱已注册".to_string()));
    }

    let code = generate_code();
    let expires_at_unix =
        (time::OffsetDateTime::now_utc() + time::Duration::minutes(5)).unix_timestamp();
    {
        let mut guard = codes.write().await;
        guard.insert(
            email.clone(),
            VerificationCodeRecord {
                code: code.clone(),
                expires_at_unix,
            },
        );
    }

    let (message, debug_code) = if email_sender.is_configured() {
        match email_sender.send_verification_code(&email, &code).await {
            Ok(()) => {
                tracing::info!("verification code sent via SMTP to {email}");
                ("验证码已发送，请检查邮箱".to_string(), None)
            }
            Err(e) => {
                tracing::warn!(
                    "SMTP send failed for {email}: {e}; returning debugCode as fallback"
                );
                (
                    "邮件发送失败，已通过响应返回调试验证码".to_string(),
                    Some(code),
                )
            }
        }
    } else {
        (
            "验证码已生成（开发模式：通过响应返回）".to_string(),
            Some(code),
        )
    };

    Ok(Json(ApiResponse {
        success: true,
        data: None,
        message: Some(message),
        debug_code,
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
        let guard = codes.read().await;
        let stored = guard
            .get(&email)
            .cloned()
            .ok_or_else(|| AppError::InvalidInput("请先发送验证码".to_string()))?;
        drop(guard);

        if time::OffsetDateTime::now_utc().unix_timestamp() > stored.expires_at_unix {
            codes.write().await.remove(&email);
            return Err(AppError::InvalidInput(
                "验证码已过期，请重新发送".to_string(),
            ));
        }

        if stored.code != verification_code {
            return Err(AppError::InvalidInput("验证码错误".to_string()));
        }
    }

    let user = User {
        id: UserId(Uuid::new_v4()),
        name: username,
        email: email.clone(),
        password,
        avatar: String::new(),
        // 新注册一律是普通用户；首任管理员靠 init.sql / ensure_schema 的 UPDATE Tanhhhhtjy 完成。
        role: "user".to_string(),
    };
    user_repo.register(user).await?;

    // 仅在注册插入成功后才消费验证码，避免唯一约束等错误导致用户无法重试。
    codes.write().await.remove(&email);

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
    let account = validate_login_account(&payload.email)?;
    let password = validate_password(&payload.password, "请输入邮箱和密码")?;

    let user = if looks_like_email(&account) {
        user_repo.find_by_email(&account.to_lowercase()).await?
    } else {
        user_repo.find_by_name(&account).await?
    };

    if let Some(user) = user {
        let is_valid = UserRepository::check_password(&password, &user.password);
        if !is_valid {
            return Err(AppError::InvalidInput("账号或密码错误".to_string()));
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
        Err(AppError::InvalidInput("账号或密码错误".to_string()))
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

#[tracing::instrument]
pub async fn update_email(
    TokenClaims { user_id, .. }: TokenClaims,
    State(user_repo): State<Arc<UserRepository>>,
    State(codes): State<Arc<RwLock<std::collections::HashMap<String, VerificationCodeRecord>>>>,
    Json(payload): Json<UpdateEmailRequest>,
) -> Result<Json<ApiResponse<UserDto>>, AppError> {
    let new_email = validate_email(&payload.new_email)?;
    let verification_code = payload.verification_code.trim().to_string();

    if verification_code.is_empty() {
        return Err(AppError::InvalidInput("请先发送验证码".to_string()));
    }

    let current = user_repo
        .find_by_id(&user_id)
        .await?
        .ok_or(AppError::Unauthorized("未登录".to_string()))?;

    if current.email == new_email {
        return Err(AppError::InvalidInput("新邮箱与当前邮箱相同".to_string()));
    }

    {
        let guard = codes.read().await;
        let stored = guard
            .get(&new_email)
            .cloned()
            .ok_or_else(|| AppError::InvalidInput("请先发送验证码".to_string()))?;
        drop(guard);

        if time::OffsetDateTime::now_utc().unix_timestamp() > stored.expires_at_unix {
            codes.write().await.remove(&new_email);
            return Err(AppError::InvalidInput(
                "验证码已过期，请重新发送".to_string(),
            ));
        }

        if stored.code != verification_code {
            return Err(AppError::InvalidInput("验证码错误".to_string()));
        }
    }

    let updated = user_repo.update_email(&user_id, &new_email).await?;

    // 仅在更新成功后消费验证码，避免唯一约束冲突等情况下用户需要重发。
    codes.write().await.remove(&new_email);

    Ok(Json(ApiResponse::success(UserDto::from_user(updated))))
}

const AVATAR_MAX_BYTES: usize = 2 * 1024 * 1024;

pub fn extension_for_mime(mime: &str) -> Option<&'static str> {
    match mime {
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/webp" => Some("webp"),
        _ => None,
    }
}

#[tracing::instrument(skip(multipart, upload_dir))]
pub async fn update_avatar(
    TokenClaims { user_id, .. }: TokenClaims,
    State(user_repo): State<Arc<UserRepository>>,
    State(upload_dir): State<crate::state::UploadDir>,
    mut multipart: Multipart,
) -> Result<Json<ApiResponse<UserDto>>, AppError> {
    let mut field = multipart
        .next_field()
        .await
        .map_err(|e| AppError::InvalidInput(format!("解析上传失败：{e}")))?
        .ok_or_else(|| AppError::InvalidInput("缺少上传文件".to_string()))?;

    let content_type = field.content_type().map(str::to_string).unwrap_or_default();
    let extension = extension_for_mime(&content_type)
        .ok_or_else(|| AppError::InvalidInput("仅支持 png / jpeg / webp 格式".to_string()))?;

    let mut bytes = Vec::with_capacity(8192);
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|e| AppError::InvalidInput(format!("读取上传内容失败：{e}")))?
    {
        if bytes.len() + chunk.len() > AVATAR_MAX_BYTES {
            return Err(AppError::InvalidInput("头像不能超过 2MB".to_string()));
        }
        bytes.extend_from_slice(&chunk);
    }

    if bytes.is_empty() {
        return Err(AppError::InvalidInput("上传文件为空".to_string()));
    }

    let avatars_dir = upload_dir.0.join("avatars");
    tokio::fs::create_dir_all(&avatars_dir)
        .await
        .map_err(|e| AppError::InvalidInput(format!("创建上传目录失败：{e}")))?;

    let filename = format!("{}.{extension}", Uuid::new_v4());
    let path = avatars_dir.join(&filename);
    tokio::fs::write(&path, &bytes)
        .await
        .map_err(|e| AppError::InvalidInput(format!("写入文件失败：{e}")))?;

    let relative_url = format!("/static/avatars/{filename}");

    let previous = user_repo.find_by_id(&user_id).await?;
    let updated = user_repo.update_avatar(&user_id, &relative_url).await?;

    // best-effort 删除旧头像，失败仅记日志，不影响接口结果
    if let Some(prev) = previous
        && !prev.avatar.is_empty()
        && let Some(name) = prev.avatar.strip_prefix("/static/avatars/")
    {
        let old_path = avatars_dir.join(name);
        if let Err(e) = tokio::fs::remove_file(&old_path).await {
            tracing::warn!("failed to delete old avatar {}: {e}", old_path.display());
        }
    }

    Ok(Json(ApiResponse::success(UserDto::from_user(updated))))
}

#[tracing::instrument(skip(email_sender))]
pub async fn password_reset_code(
    State(user_repo): State<Arc<UserRepository>>,
    State(codes): State<Arc<RwLock<std::collections::HashMap<String, VerificationCodeRecord>>>>,
    State(email_sender): State<EmailSender>,
    Json(payload): Json<PasswordResetCodeRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let email = validate_email(&payload.email)?;

    // 与 send_verification_code 相反：这里要求邮箱已注册
    if user_repo.find_by_email(&email).await?.is_none() {
        return Err(AppError::InvalidInput("该邮箱未注册".to_string()));
    }

    let code = generate_code();
    let expires_at_unix =
        (time::OffsetDateTime::now_utc() + time::Duration::minutes(5)).unix_timestamp();
    {
        let mut guard = codes.write().await;
        guard.insert(
            email.clone(),
            VerificationCodeRecord {
                code: code.clone(),
                expires_at_unix,
            },
        );
    }

    let (message, debug_code) = if email_sender.is_configured() {
        match email_sender.send_verification_code(&email, &code).await {
            Ok(()) => {
                tracing::info!("password reset code sent via SMTP to {email}");
                ("验证码已发送，请检查邮箱".to_string(), None)
            }
            Err(e) => {
                tracing::warn!(
                    "SMTP send failed for {email}: {e}; returning debugCode as fallback"
                );
                (
                    "邮件发送失败，已通过响应返回调试验证码".to_string(),
                    Some(code),
                )
            }
        }
    } else {
        (
            "验证码已生成（开发模式：通过响应返回）".to_string(),
            Some(code),
        )
    };

    Ok(Json(ApiResponse {
        success: true,
        data: None,
        message: Some(message),
        debug_code,
    }))
}

#[tracing::instrument]
pub async fn password_reset(
    State(user_repo): State<Arc<UserRepository>>,
    State(codes): State<Arc<RwLock<std::collections::HashMap<String, VerificationCodeRecord>>>>,
    Json(payload): Json<PasswordResetRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let email = validate_email(&payload.email)?;
    let verification_code = payload.verification_code.trim().to_string();
    let new_password = validate_password(&payload.new_password, "请填写新密码")?;

    if verification_code.is_empty() {
        return Err(AppError::InvalidInput("请先发送验证码".to_string()));
    }

    let user = user_repo
        .find_by_email(&email)
        .await?
        .ok_or_else(|| AppError::InvalidInput("该邮箱未注册".to_string()))?;

    {
        let guard = codes.read().await;
        let stored = guard
            .get(&email)
            .cloned()
            .ok_or_else(|| AppError::InvalidInput("请先发送验证码".to_string()))?;
        drop(guard);

        if time::OffsetDateTime::now_utc().unix_timestamp() > stored.expires_at_unix {
            codes.write().await.remove(&email);
            return Err(AppError::InvalidInput(
                "验证码已过期，请重新发送".to_string(),
            ));
        }

        if stored.code != verification_code {
            return Err(AppError::InvalidInput("验证码错误".to_string()));
        }
    }

    user_repo.update_password(&user.id, &new_password).await?;

    // 仅在改密成功后才消费验证码
    codes.write().await.remove(&email);

    Ok(Json(ApiResponse::success_without_data(Some(
        "密码已重置，请用新密码登录".to_string(),
    ))))
}

#[cfg(test)]
mod tests {
    use super::{UserDto, extension_for_mime, looks_like_email, validate_login_account};
    use crate::domain::user::{User, UserId};
    use uuid::Uuid;

    #[test]
    fn looks_like_email_detects_at_symbol() {
        assert!(looks_like_email("a@b.com"));
        assert!(looks_like_email("foo@bar"));
    }

    #[test]
    fn looks_like_email_rejects_plain_username() {
        assert!(!looks_like_email("alice"));
        assert!(!looks_like_email("user_123"));
    }

    #[test]
    fn validate_login_account_rejects_empty() {
        assert!(validate_login_account("").is_err());
        assert!(validate_login_account("   ").is_err());
    }

    #[test]
    fn validate_login_account_preserves_case() {
        assert_eq!(validate_login_account("Alice").unwrap(), "Alice");
        assert_eq!(validate_login_account("  Bob  ").unwrap(), "Bob");
    }

    #[test]
    fn extension_for_mime_accepts_supported_image_types() {
        assert_eq!(extension_for_mime("image/png"), Some("png"));
        assert_eq!(extension_for_mime("image/jpeg"), Some("jpg"));
        assert_eq!(extension_for_mime("image/jpg"), Some("jpg"));
        assert_eq!(extension_for_mime("image/webp"), Some("webp"));
    }

    #[test]
    fn extension_for_mime_rejects_other_types() {
        assert_eq!(extension_for_mime("image/gif"), None);
        assert_eq!(extension_for_mime("application/pdf"), None);
        assert_eq!(extension_for_mime(""), None);
    }

    #[test]
    fn generate_code_returns_six_digits() {
        for _ in 0..50 {
            let code = super::generate_code();
            assert_eq!(code.len(), 6);
            assert!(code.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn generate_code_is_not_deterministic() {
        let mut codes = std::collections::HashSet::new();
        for _ in 0..50 {
            codes.insert(super::generate_code());
        }
        // 50 个调用拿到 50 个全相同结果的概率是 (1/10^6)^49，实际上一定不会发生
        assert!(codes.len() > 1, "generate_code 应该返回随机序列");
    }

    #[test]
    fn user_dto_from_user_preserves_role_for_admin_gating() {
        let user = User {
            id: UserId(Uuid::new_v4()),
            name: "Admin".to_string(),
            email: "admin@example.com".to_string(),
            password: "hashed".to_string(),
            avatar: "/avatar.png".to_string(),
            role: "admin".to_string(),
        };

        let dto = UserDto::from_user(user);

        assert_eq!(dto.username, "Admin");
        assert_eq!(dto.role, "admin");
        assert!(dto.token.is_none());
    }

    #[test]
    fn user_dto_with_token_serializes_role_and_token_together() {
        let user = User {
            id: UserId(Uuid::new_v4()),
            name: "Admin".to_string(),
            email: "admin@example.com".to_string(),
            password: "hashed".to_string(),
            avatar: "/avatar.png".to_string(),
            role: "admin".to_string(),
        };

        let dto = UserDto::from_user_with_token(user, "jwt-token".to_string());
        let json = serde_json::to_value(dto).unwrap();

        assert_eq!(json.get("role").and_then(|v| v.as_str()), Some("admin"));
        assert_eq!(
            json.get("token").and_then(|v| v.as_str()),
            Some("jwt-token")
        );
    }
}
