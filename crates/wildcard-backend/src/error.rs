use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;
use tracing::error;
use wildcard_backend_macros::StatusCode;

#[derive(Serialize)]
struct ErrorResponse {
    success: bool,
    message: String,
}

#[derive(Debug, Error, StatusCode)]
pub enum AppError {
    #[status(StatusCode::NOT_FOUND)]
    #[error("资源未找到")]
    NotFound,

    #[status(StatusCode::BAD_REQUEST)]
    #[error("输入无效：{0}")]
    InvalidInput(String),

    #[status(StatusCode::BAD_REQUEST)]
    #[error("上传失败：{0}")]
    UploadFailed(#[from] axum::extract::multipart::MultipartError),

    #[status(StatusCode::INTERNAL_SERVER_ERROR)]
    #[error("数据库错误：{0}")]
    DatabaseSql(#[from] sqlx::Error),

    #[status(StatusCode::INTERNAL_SERVER_ERROR)]
    #[error("数据库错误：{0}")]
    DatabaseRedis(#[from] redis::RedisError),

    #[status(StatusCode::INTERNAL_SERVER_ERROR)]
    #[error("数据库错误：{0}")]
    DatabaseRedisPool(#[from] deadpool_redis::PoolError),

    #[status(StatusCode::CONFLICT)]
    #[error("用户已存在：{0}")]
    UserAlreadyExist(String),

    #[status(StatusCode::UNAUTHORIZED)]
    #[error("权限不足：{0}")]
    Unauthorized(String),

    #[status(StatusCode::UNAUTHORIZED)]
    #[error("登录密码错误")]
    InvalidPassword,

    #[status(StatusCode::INTERNAL_SERVER_ERROR)]
    #[error("邮件发送失败：{0}")]
    Email(String),

    #[status(StatusCode::INTERNAL_SERVER_ERROR)]
    #[error("加密错误")]
    Crypto(#[from] password_hash::Error),

    #[status(StatusCode::INTERNAL_SERVER_ERROR)]
    #[error("文件读写错误：{0}")]
    File(#[from] std::io::Error),

    #[status(StatusCode::INTERNAL_SERVER_ERROR)]
    #[error("线程池执行任务失败：{0}")]
    Join(#[from] tokio::task::JoinError),

    #[status(StatusCode::BAD_REQUEST)]
    #[error("JSON 解析错误：{0}")]
    Json(#[from] serde_json::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = Json(ErrorResponse {
            success: false,
            message: self.to_string(),
        });
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            error!("Encounter internal server error: {}", self);
        }
        (status, body).into_response()
    }
}
