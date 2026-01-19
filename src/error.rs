use axum::{Json, http::StatusCode, response::IntoResponse};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorObject,
}

#[derive(Debug, Serialize)]
pub struct ErrorObject {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(thiserror::Error, Debug)]
pub enum ApiError {
    #[error("unauthorized")]
    Unauthorized(&'static str, String),
    #[error("forbidden")]
    Forbidden(&'static str, String),
    #[error("bad request")]
    BadRequest(&'static str, String),
    #[error("internal error")]
    Internal(String),
}

impl ApiError {
    pub fn invalid_credentials() -> Self {
        ApiError::Unauthorized(
            "INVALID_CREDENTIALS",
            "Username or password is incorrect".into(),
        )
    }
    pub fn session_expired() -> Self {
        ApiError::Unauthorized("SESSION_EXPIRED", "Session is invalid or expired".into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, code, message) = match self {
            ApiError::Unauthorized(code, msg) => (StatusCode::UNAUTHORIZED, code, msg),
            ApiError::Forbidden(code, msg) => (StatusCode::FORBIDDEN, code, msg),
            ApiError::BadRequest(code, msg) => (StatusCode::BAD_REQUEST, code, msg),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", msg),
        };

        let body = ErrorResponse {
            error: ErrorObject {
                code,
                message,
                details: None,
            },
        };

        (status, Json(body)).into_response()
    }
}
