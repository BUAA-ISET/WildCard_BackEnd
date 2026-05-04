use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;

#[derive(Serialize)]
struct ErrorResponse {
    message: String,
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("资源未找到")]
    NotFound,
    #[error("输入无效：{0}")]
    InvalidInput(String),
    #[error("数据库错误：{0}")]
    DatabaseError(#[from] sqlx::Error),
    #[error("账号或密码错误")]
    InvalidPassword,
    #[error("用户已存在")]
    UserAlreadyExist(String),
    #[error("没有登录")]
    Unauthorized(String),
    #[error("加密错误")]
    CryptoError(#[from] password_hash::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::InvalidInput(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::UserAlreadyExist(msg) => (StatusCode::CONFLICT, msg),
            AppError::InvalidPassword => (StatusCode::UNAUTHORIZED, self.to_string()),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            AppError::DatabaseError(error) => {
                (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
            }
            AppError::CryptoError(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
        };
        let body = Json(ErrorResponse {
            message: error_message,
        });
        (status, body).into_response()
    }
}
