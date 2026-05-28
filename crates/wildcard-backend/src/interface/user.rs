use super::auth::TokenClaims;
use crate::domain::email::MailAddress;
use crate::infrastructure::email::EmailSender;
use crate::infrastructure::user::UserRepository;
use crate::{
    domain::user::{User, UserId},
    state::JwtSecret,
};
use crate::{error::AppError, state::UploadDir};
use axum::{
    Json,
    extract::{Multipart, Query, State},
    http::{HeaderMap, header},
};
use axum_extra::extract::cookie::{Cookie, SameSite};
use jsonwebtoken::{EncodingKey, Header};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

fn validate_email(email: &str) -> Result<MailAddress, AppError> {
    if email.is_empty() {
        return Err(AppError::InvalidInput("请输入邮箱".to_string()));
    }
    email
        .parse()
        .map_err(|_| AppError::InvalidInput("邮箱格式不正确".to_string()))
}

fn validate_login_account(account: &str) -> Result<String, AppError> {
    let account = account.trim().to_string();
    if account.is_empty() {
        return Err(AppError::InvalidInput("请输入邮箱或用户名".to_string()));
    }
    Ok(account)
}

fn validate_user_name(user_name: &str) -> Result<String, AppError> {
    let user_name = user_name.trim().to_string();
    if user_name.is_empty() {
        return Err(AppError::InvalidInput("用户名不能为空".to_string()));
    }
    Ok(user_name)
}

fn validate_password(password: &str) -> Result<String, AppError> {
    let password = password.trim().to_string();
    if password.is_empty() {
        return Err(AppError::InvalidInput("密码不能为空".to_string()));
    }
    Ok(password)
}

#[derive(Debug, Deserialize)]
pub struct FindUserRequest {
    pub user_name: String,
}

#[tracing::instrument(skip(user_repo))]
pub async fn find(
    State(user_repo): State<Arc<UserRepository>>,
    Query(FindUserRequest { user_name }): Query<FindUserRequest>,
) -> Result<Json<User>, AppError> {
    user_repo
        .find_by_name(&user_name)
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}

#[derive(Debug, Deserialize)]
pub struct VerificationCodeRequest {
    pub email: String,
}

#[tracing::instrument(skip(email_sender))]
pub async fn verification_code(
    State(user_repo): State<Arc<UserRepository>>,
    State(email_sender): State<Arc<EmailSender>>,
    Json(VerificationCodeRequest { email }): Json<VerificationCodeRequest>,
) -> Result<(), AppError> {
    let email = validate_email(&email)?;

    // 要求邮箱已注册
    if user_repo.find_by_email(&email).await?.is_none() {
        return Err(AppError::InvalidInput("该邮箱未注册".to_string()));
    }

    let code = user_repo.generate_code(&email).await?;

    tokio::spawn(async move {
        email_sender.send_verification_code(email, code).await.ok();
    });

    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub user_name: String,
    pub password: String,
    pub verification_code: String,
}

#[tracing::instrument(skip(user_repo, password))]
pub async fn register(
    State(user_repo): State<Arc<UserRepository>>,
    Json(RegisterRequest {
        email,
        user_name,
        password,
        verification_code,
    }): Json<RegisterRequest>,
) -> Result<(), AppError> {
    let email = validate_email(&email)?;
    let user_name = validate_user_name(&user_name)?;
    let password = validate_password(&password)?;
    user_repo.verify_code(&email, &verification_code).await?;

    let user = User {
        id: UserId(Uuid::new_v4()),
        name: user_name,
        email: email.clone(),
        password,
        avatar: String::new(),
    };
    user_repo.register(user).await
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub account: String,
    pub password: String,
}

#[tracing::instrument(skip(jwt_secret, user_repo, password))]
pub async fn login(
    State(JwtSecret(jwt_secret)): State<JwtSecret>,
    State(user_repo): State<Arc<UserRepository>>,
    Json(LoginRequest { account, password }): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<User>), AppError> {
    let account = validate_login_account(&account)?;
    let password = validate_password(&password)?;

    let try_email = validate_email(&account);
    let user = if let Ok(email) = try_email {
        user_repo.find_by_email(&email).await?
    } else {
        user_repo.find_by_name(&account).await?
    };

    if let Some(user) = user {
        let is_valid = UserRepository::check_password(&password, &user.password);
        if !is_valid {
            return Err(AppError::InvalidPassword);
        }

        let now = time::OffsetDateTime::now_utc();
        let exp = now + time::Duration::days(1);

        let claims = TokenClaims {
            user_id: user.id.to_owned(),
            exp: exp.unix_timestamp() as usize,
            iat: now.unix_timestamp() as usize,
        };

        let token = jsonwebtoken::encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&jwt_secret),
        )
        .unwrap();

        let cookie = Cookie::build(("token", &token))
            .path("/")
            .max_age(time::Duration::days(1))
            .same_site(SameSite::Lax)
            .http_only(true)
            .build();

        let mut headers = HeaderMap::new();
        headers.insert(header::SET_COOKIE, cookie.to_string().parse().unwrap());

        Ok((headers, Json(user)))
    } else {
        Err(AppError::InvalidPassword)
    }
}

#[tracing::instrument]
pub async fn logout() -> HeaderMap {
    let cookie = Cookie::build(("token", ""))
        .path("/")
        .max_age(time::Duration::days(-1))
        .same_site(SameSite::Lax)
        .http_only(true)
        .build();

    let mut header = HeaderMap::new();
    header.insert(header::SET_COOKIE, cookie.to_string().parse().unwrap());
    header
}

#[tracing::instrument(skip(user_repo))]
pub async fn current(
    TokenClaims { user_id, .. }: TokenClaims,
    State(user_repo): State<Arc<UserRepository>>,
) -> Result<Json<User>, AppError> {
    user_repo
        .find_by_id(&user_id)
        .await?
        .map(Json)
        .ok_or(AppError::Unauthorized("".to_string()))
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserNameRequest {
    pub user_name: String,
}

#[tracing::instrument(skip(user_repo))]
pub async fn update_user_name(
    TokenClaims { user_id, .. }: TokenClaims,
    State(user_repo): State<Arc<UserRepository>>,
    Json(UpdateUserNameRequest { user_name }): Json<UpdateUserNameRequest>,
) -> Result<Json<User>, AppError> {
    let user_name = validate_user_name(&user_name)?;
    let user = user_repo.update_user_name(&user_id, &user_name).await?;

    Ok(Json(user))
}

#[derive(Debug, Deserialize)]
pub struct UpdatePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[tracing::instrument(skip(user_repo, current_password, new_password))]
pub async fn update_password(
    TokenClaims { user_id, .. }: TokenClaims,
    State(user_repo): State<Arc<UserRepository>>,
    Json(UpdatePasswordRequest {
        current_password,
        new_password,
    }): Json<UpdatePasswordRequest>,
) -> Result<(), AppError> {
    let current_password = validate_password(&current_password)?;
    let new_password = validate_password(&new_password)?;

    let user = user_repo
        .find_by_id(&user_id)
        .await?
        .ok_or(AppError::Unauthorized("用户不存在".to_string()))?;

    if !UserRepository::check_password(&current_password, &user.password) {
        return Err(AppError::InvalidInput("当前密码错误".to_string()));
    }

    user_repo.update_password(&user_id, &new_password).await?;

    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct UpdateEmailRequest {
    pub new_email: String,
    pub verification_code: String,
}

#[tracing::instrument(skip(user_repo))]
pub async fn update_email(
    TokenClaims { user_id, .. }: TokenClaims,
    State(user_repo): State<Arc<UserRepository>>,
    Json(UpdateEmailRequest {
        new_email,
        verification_code,
    }): Json<UpdateEmailRequest>,
) -> Result<Json<User>, AppError> {
    let new_email = validate_email(&new_email)?;
    user_repo
        .verify_code(&new_email, &verification_code)
        .await?;

    let updated = user_repo.update_email(&user_id, &new_email).await?;

    Ok(Json(updated))
}

const AVATAR_MAX_BYTES: usize = 2 * 1024 * 1024;

#[tracing::instrument(skip(multipart, upload_dir))]
pub async fn update_avatar(
    TokenClaims { user_id, .. }: TokenClaims,
    State(user_repo): State<Arc<UserRepository>>,
    State(upload_dir): State<UploadDir>,
    mut multipart: Multipart,
) -> Result<Json<User>, AppError> {
    let mut field = multipart
        .next_field()
        .await?
        .ok_or_else(|| AppError::InvalidInput("缺少上传文件".to_string()))?;

    let mime = field.content_type().map(str::to_string).unwrap_or_default();

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

    let updated_user = user_repo
        .update_avatar(upload_dir, &user_id, bytes, &mime)
        .await?;

    Ok(Json(updated_user))
}

#[derive(Debug, Deserialize)]
pub struct PasswordResetRequest {
    pub email: String,
    pub verification_code: String,
    pub new_password: String,
}

#[tracing::instrument(skip(user_repo, new_password))]
pub async fn password_reset(
    State(user_repo): State<Arc<UserRepository>>,
    Json(PasswordResetRequest {
        email,
        verification_code,
        new_password,
    }): Json<PasswordResetRequest>,
) -> Result<(), AppError> {
    let email = validate_email(&email)?;
    let new_password = validate_password(&new_password)?;
    let user = user_repo
        .find_by_email(&email)
        .await?
        .ok_or_else(|| AppError::InvalidInput("该邮箱未注册".to_string()))?;
    user_repo.verify_code(&email, &verification_code).await?;

    user_repo.update_password(&user.id, &new_password).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
