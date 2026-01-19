// src/routes/clinic_routes.rs

use axum::{
    extract::State,
    routing::{get, patch},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    error::ApiError,
    middleware::auth_context::AuthContext,
    models::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/clinic", get(get_clinic))
        .route("/clinic", patch(update_clinic))
}

#[derive(Debug, Serialize)]
pub struct ClinicResponse {
    pub data: ClinicData,
}

#[derive(Debug, Serialize)]
pub struct ClinicData {
    pub clinic_name: String,
}

pub async fn get_clinic(
    State(state): State<AppState>,
    _auth: AuthContext, // require login for now (consistent + simplest)
) -> Result<Json<ClinicResponse>, ApiError> {
    let clinic_name: Option<String> = sqlx::query_scalar(
        r#"
        SELECT clinic_name
        FROM clinic_settings
        WHERE singleton_id = TRUE
        "#,
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(ClinicResponse {
        data: ClinicData {
            clinic_name: clinic_name.unwrap_or_else(|| "Clinic".to_string()),
        },
    }))
}

#[derive(Debug, Deserialize)]
pub struct UpdateClinicRequest {
    pub clinic_name: String,
}

fn ensure_admin_or_manager(auth: &AuthContext) -> Result<(), ApiError> {
    // roles: 1 admin, 2 manager
    if auth.role == 1 || auth.role == 2 {
        Ok(())
    } else {
        Err(ApiError::Forbidden(
            "FORBIDDEN",
            "Only admin/manager can update clinic settings".into(),
        ))
    }
}

pub async fn update_clinic(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(req): Json<UpdateClinicRequest>,
) -> Result<Json<ClinicResponse>, ApiError> {
    ensure_admin_or_manager(&auth)?;

    let name = req.clinic_name.trim();
    if name.is_empty() {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "clinic_name is required".into(),
        ));
    }
    if name.len() > 128 {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "clinic_name is too long (max 128)".into(),
        ));
    }

    // Upsert singleton row (safe even if missing)
    let clinic_name: String = sqlx::query_scalar(
        r#"
        INSERT INTO clinic_settings (singleton_id, clinic_name)
        VALUES (TRUE, $1)
        ON CONFLICT (singleton_id)
        DO UPDATE SET clinic_name = EXCLUDED.clinic_name
        RETURNING clinic_name
        "#,
    )
    .bind(name)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(ClinicResponse {
        data: ClinicData { clinic_name },
    }))
}
