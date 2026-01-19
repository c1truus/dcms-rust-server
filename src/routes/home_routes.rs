use axum::{Json, Router, extract::State, routing::get};

use crate::error::ApiError;
use crate::middleware::auth_context::AuthContext;
use crate::models::AppState;

#[derive(serde::Serialize)]
pub struct HomeResponse {
    pub data: HomeData,
}

#[derive(serde::Serialize)]
pub struct HomeData {
    pub view: String,
    pub message: String,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/home", get(home))
}

pub async fn home(
    State(_state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<HomeResponse>, ApiError> {
    // DB stores a single role (smallint):
    // 0 patient, 1 admin, 2 manager, 3 doctor, 4 receptionist
    let view = match auth.role {
        1 => "admin",
        2 => "manager",
        3 => "doctor",
        4 => "receptionist",
        0 => "patient",
        _ => "unknown",
    };

    Ok(Json(HomeResponse {
        data: HomeData {
            view: view.to_string(),
            message: "placeholder home payload (role-based)".to_string(),
        },
    }))
}
