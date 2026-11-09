//! Typed errors returned by RPC procedures.
//!
//! Every procedure returns `Result<T, RpcError>`. The error is serialized as
//! `{ "code": "NOT_FOUND", "message": "..." }` with a matching HTTP status, and
//! the same shape is generated for the TypeScript client.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    BadRequest,
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    Internal,
}

impl ErrorCode {
    pub fn status(self) -> StatusCode {
        match self {
            ErrorCode::BadRequest => StatusCode::BAD_REQUEST,
            ErrorCode::Unauthorized => StatusCode::UNAUTHORIZED,
            ErrorCode::Forbidden => StatusCode::FORBIDDEN,
            ErrorCode::NotFound => StatusCode::NOT_FOUND,
            ErrorCode::Conflict => StatusCode::CONFLICT,
            ErrorCode::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, thiserror::Error)]
#[error("{code:?}: {message}")]
pub struct RpcError {
    pub code: ErrorCode,
    pub message: String,
}

impl RpcError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::BadRequest, message)
    }
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unauthorized, message)
    }
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Forbidden, message)
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, message)
    }
    /// Internal errors never leak details to the client; the cause is logged.
    pub fn internal(cause: impl std::fmt::Display) -> Self {
        tracing::error!(error = %cause, "internal error");
        Self::new(ErrorCode::Internal, "internal server error")
    }
}

impl From<sqlx::Error> for RpcError {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::RowNotFound => RpcError::not_found("not found"),
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                RpcError::conflict("already exists")
            }
            _ => RpcError::internal(err),
        }
    }
}

impl From<anyhow::Error> for RpcError {
    fn from(err: anyhow::Error) -> Self {
        match err.downcast::<RpcError>() {
            Ok(rpc) => rpc,
            Err(err) => RpcError::internal(format!("{err:#}")),
        }
    }
}

impl IntoResponse for RpcError {
    fn into_response(self) -> Response {
        (self.code.status(), axum::Json(self)).into_response()
    }
}

/// Return early with a `BAD_REQUEST` error unless the condition holds.
///
/// ```ignore
/// northstack::ensure!(!input.title.trim().is_empty(), "title must not be empty");
/// ```
#[macro_export]
macro_rules! ensure {
    ($cond:expr, $($msg:tt)+) => {
        if !($cond) {
            return Err($crate::RpcError::bad_request(format!($($msg)+)));
        }
    };
}
