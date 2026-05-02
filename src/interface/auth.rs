use crate::{domain::user::UserId, error::AppError, state::JwtSecret};
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{self},
};
use axum_extra::extract::CookieJar;
use jsonwebtoken::{self, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenClaims {
    #[serde(rename = "sub")]
    pub user_id: UserId,
    pub iat: usize,
    pub exp: usize,
}

impl<S> FromRequestParts<S> for TokenClaims
where
    S: Send + Sync,
    JwtSecret: FromRef<S>,
{
    type Rejection = AppError;
    async fn from_request_parts(
        parts: &mut http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let JwtSecret(jwt_secret) = JwtSecret::from_ref(state);

        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get("token")
            .map(|cookie| cookie.value())
            .ok_or(AppError::Unauthorized("未找到登录标签".to_string()))?;

        let token_data = jsonwebtoken::decode::<TokenClaims>(
            token,
            &DecodingKey::from_secret(&jwt_secret),
            &Validation::default(),
        )
        .map_err(|_| AppError::Unauthorized("无效的 Token".to_string()))?;

        Ok(token_data.claims)
    }
}
