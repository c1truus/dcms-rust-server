use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorObject,
}

#[derive(Debug, Serialize)]
pub struct ErrorObject {
    pub code: String,
    pub message: String,
}

#[derive(Debug)]
pub enum ApiError {
    Unauthorized(&'static str, String),
    Forbidden(&'static str, String),
    BadRequest(&'static str, String),
    #[allow(dead_code)]
    NotFound(&'static str, String),
    #[allow(dead_code)]
    Conflict(&'static str, String),
    Internal(String),
}

impl ApiError {
    pub fn invalid_credentials() -> Self {
        ApiError::Unauthorized("INVALID_CREDENTIALS", "Username or password is incorrect".into())
    }

    pub fn session_expired() -> Self {
        ApiError::Unauthorized("SESSION_EXPIRED", "Session expired".into())
    }

    fn to_error_response(code: &str, message: &str) -> Json<ErrorResponse> {
        Json(ErrorResponse {
            error: ErrorObject {
                code: code.to_string(),
                message: message.to_string(),
            },
        })
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::Unauthorized(code, msg) => {
                (StatusCode::UNAUTHORIZED, ApiError::to_error_response(code, &msg)).into_response()
            }
            ApiError::Forbidden(code, msg) => {
                (StatusCode::FORBIDDEN, ApiError::to_error_response(code, &msg)).into_response()
            }
            ApiError::BadRequest(code, msg) => {
                (StatusCode::BAD_REQUEST, ApiError::to_error_response(code, &msg)).into_response()
            }
            ApiError::NotFound(code, msg) => {
                (StatusCode::NOT_FOUND, ApiError::to_error_response(code, &msg)).into_response()
            }
            ApiError::Conflict(code, msg) => {
                (StatusCode::CONFLICT, ApiError::to_error_response(code, &msg)).into_response()
            }
            ApiError::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiError::to_error_response("INTERNAL", &msg),
            )
                .into_response(),
        }
    }
}
