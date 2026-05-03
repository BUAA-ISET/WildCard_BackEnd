use super::auth::TokenClaims;
use crate::error::AppError;
use crate::infrastructure::user::UserRepository;
use crate::{
    domain::user::{User, UserId},
    state::JwtSecret,
};
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, header},
};
use axum_extra::extract::cookie::{Cookie, SameSite};
use jsonwebtoken::{EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub user_name: String,
    pub password: String,
}

#[tracing::instrument]
pub async fn register_handler(
    State(user_repo): State<Arc<UserRepository>>,
    Json(RegisterRequest {
        user_name,
        email,
        password,
    }): Json<RegisterRequest>,
) -> Result<(), AppError> {
    let user = User::new(user_name, password, email);
    user_repo.register(user).await
}

#[derive(Debug, Deserialize)]
pub struct FindRequest {
    pub user_name: String,
}

#[derive(Debug, Serialize)]
pub struct FindResponse {
    pub user_id: UserId,
    pub user_name: String,
    pub email: String,
}

impl From<User> for FindResponse {
    fn from(user: User) -> Self {
        Self {
            user_id: user.id,
            user_name: user.name,
            email: user.email,
        }
    }
}

#[tracing::instrument]
pub async fn find_handler(
    State(user_repo): State<Arc<UserRepository>>,
    Query(FindRequest { user_name }): Query<FindRequest>,
) -> Result<Json<FindResponse>, AppError> {
    user_repo
        .find_by_name(&user_name)
        .await?
        .map(FindResponse::from)
        .map(Json)
        .ok_or(AppError::NotFound)
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub user_name: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
}

#[tracing::instrument]
pub async fn login_handler(
    State(JwtSecret(jwt_secret)): State<JwtSecret>,
    State(user_repo): State<Arc<UserRepository>>,
    Json(LoginRequest {
        user_name,
        password,
    }): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<LoginResponse>), AppError> {
    let user = user_repo.find_by_name(&user_name).await?;

    if let Some(user) = user {
        let is_valid = UserRepository::check_password(&password, &user.password);
        if !is_valid {
            return Err(AppError::InvalidPassword);
        }

        let now = time::OffsetDateTime::now_utc();
        let exp = now + time::Duration::days(1);

        let claims = TokenClaims {
            user_id: user.id,
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

        Ok((headers, Json(LoginResponse { token })))
    } else {
        Err(AppError::InvalidPassword)
    }
}

#[tracing::instrument]
pub async fn logout_handler() -> HeaderMap {
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

#[tracing::instrument]
pub async fn me(
    TokenClaims { user_id, .. }: TokenClaims,
    State(user_repo): State<Arc<UserRepository>>,
) -> Result<Json<FindResponse>, AppError> {
    user_repo
        .find_by_id(&user_id)
        .await?
        .map(FindResponse::from)
        .map(Json)
        .ok_or(AppError::Unauthorized("".to_string()))
}
