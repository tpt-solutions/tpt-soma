use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use sqlx::Error as SqlxError;
use thiserror::Error;
use tpt_soma_core::dp::BudgetError;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("database error: {0}")]
    Database(#[from] SqlxError),
    #[error("core error: {0}")]
    Core(#[from] tpt_soma_core::Error),
    #[error("internal error: {0}")]
    Internal(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("differential privacy error: {0}")]
    DifferentialPrivacy(#[from] BudgetError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            ApiError::Database(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Database error: {}", e),
            ),
            ApiError::Core(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Core error: {}", e),
            ),
            ApiError::Internal(e) => (StatusCode::INTERNAL_SERVER_ERROR, e),
            ApiError::NotFound(e) => (StatusCode::NOT_FOUND, e),
            ApiError::BadRequest(e) => (StatusCode::BAD_REQUEST, e),
            ApiError::Unauthorized(e) => (StatusCode::UNAUTHORIZED, e),
            ApiError::Forbidden(e) => (StatusCode::FORBIDDEN, e),
            ApiError::DifferentialPrivacy(e) => (
                StatusCode::FORBIDDEN,
                format!("Differential privacy error: {}", e),
            ),
            ApiError::Io(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("IO error: {}", e),
            ),
        };

        let body = Json(json!({
            "error": error_message,
        }));

        (status, body).into_response()
    }
}

pub type Result<T> = std::result::Result<T, ApiError>;
