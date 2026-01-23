// src/routes/clinic_routes.rs

use axum::{
    extract::State,
    routing::{get, patch},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::{
    error::ApiError,
    middleware::auth_context::AuthContext,
    models::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        // profile
        .route("/clinic", get(get_clinic))
        .route("/clinic", patch(update_clinic))
        // settings
        .route("/clinic/settings", get(get_clinic_settings))
        .route("/clinic/settings", patch(patch_clinic_settings))
        // meta (UI helper)
        .route("/clinic/meta", get(get_clinic_meta))
}

fn ensure_admin(auth: &AuthContext) -> Result<(), ApiError> {
    // roles: 1 admin
    if auth.role == 1 {
        Ok(())
    } else {
        Err(ApiError::Forbidden(
            "FORBIDDEN",
            "Only admin can update clinic configuration".into(),
        ))
    }
}

fn validate_timezone(tz: &str) -> Result<(), ApiError> {
    let tz = tz.trim();
    if tz.is_empty() {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "timezone is required".into(),
        ));
    }
    if tz.len() > 64 {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "timezone too long".into(),
        ));
    }
    // Soft validation only (keep it simple for now)
    // Typical examples: "Asia/Ulaanbaatar", "UTC"
    Ok(())
}

fn validate_slot_minutes(v: i32) -> Result<(), ApiError> {
    // Keep a safe allowlist so scheduling logic stays consistent.
    const ALLOWED: [i32; 7] = [5, 10, 15, 20, 30, 45, 60];
    if !ALLOWED.contains(&v) {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            format!("default_slot_minutes must be one of {:?}", ALLOWED),
        ));
    }
    Ok(())
}

fn validate_business_hours(bh: &JsonValue) -> Result<(), ApiError> {
    // Minimal shape check (you can harden later):
    // Expect object: { "mon": [{"start":"09:00","end":"18:00"}], "tue":[...], ... }
    if !bh.is_object() {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "business_hours must be a JSON object".into(),
        ));
    }
    Ok(())
}

/* ============================================================
   1) /clinic (PROFILE)
   ============================================================ */

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
    _auth: AuthContext,
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

pub async fn update_clinic(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(req): Json<UpdateClinicRequest>,
) -> Result<Json<ClinicResponse>, ApiError> {
    ensure_admin(&auth)?;

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
            "clinic_name max 128 chars".into(),
        ));
    }

    let clinic_name: String = sqlx::query_scalar(
        r#"
        INSERT INTO clinic_settings (singleton_id, clinic_name, updated_at, updated_by_user_id)
        VALUES (TRUE, $1, now(), $2)
        ON CONFLICT (singleton_id)
        DO UPDATE SET
          clinic_name = EXCLUDED.clinic_name,
          updated_at = now(),
          updated_by_user_id = EXCLUDED.updated_by_user_id
        RETURNING clinic_name
        "#,
    )
    .bind(name)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(ClinicResponse {
        data: ClinicData { clinic_name },
    }))
}

/* ============================================================
   2) /clinic/settings (OPERATIONAL SETTINGS)
   ============================================================ */

#[derive(Debug, Serialize)]
pub struct ClinicSettingsResponse {
    pub data: ClinicSettingsData,
}

#[derive(Debug, Serialize)]
pub struct ClinicSettingsData {
    pub timezone: String,
    pub default_slot_minutes: i32,
    pub business_hours: JsonValue,
    pub updated_at: String,
    pub updated_by_user_id: Option<String>,
}

pub async fn get_clinic_settings(
    State(state): State<AppState>,
    _auth: AuthContext,
) -> Result<Json<ClinicSettingsResponse>, ApiError> {
    let row = sqlx::query!(
        r#"
        SELECT
          timezone,
          default_slot_minutes,
          business_hours,
          updated_at,
          updated_by_user_id
        FROM clinic_settings
        WHERE singleton_id = TRUE
        "#
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    // If row missing (shouldn't happen due to seed), provide safe defaults.
    let (timezone, default_slot_minutes, business_hours, updated_at, updated_by_user_id) =
        if let Some(r) = row {
            (
                r.timezone,
                r.default_slot_minutes,
                r.business_hours,
                r.updated_at.to_rfc3339(),
                r.updated_by_user_id.map(|u| u.to_string()),
            )
        } else {
            (
                "UTC".to_string(),
                30,
                serde_json::json!({}),
                chrono::Utc::now().to_rfc3339(),
                None,
            )
        };

    Ok(Json(ClinicSettingsResponse {
        data: ClinicSettingsData {
            timezone,
            default_slot_minutes,
            business_hours,
            updated_at,
            updated_by_user_id,
        },
    }))
}

#[derive(Debug, Deserialize)]
pub struct PatchClinicSettingsRequest {
    pub timezone: Option<String>,
    pub default_slot_minutes: Option<i32>,
    pub business_hours: Option<JsonValue>,
}

pub async fn patch_clinic_settings(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(req): Json<PatchClinicSettingsRequest>,
) -> Result<Json<ClinicSettingsResponse>, ApiError> {
    ensure_admin(&auth)?;

    // Load current (and lock)
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    let cur = sqlx::query!(
        r#"
        SELECT timezone, default_slot_minutes, business_hours
        FROM clinic_settings
        WHERE singleton_id = TRUE
        FOR UPDATE
        "#
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    let mut timezone = cur
        .as_ref()
        .map(|r| r.timezone.clone())
        .unwrap_or_else(|| "UTC".into());

    let mut default_slot_minutes = cur
        .as_ref()
        .map(|r| r.default_slot_minutes)
        .unwrap_or(30);

    let mut business_hours = cur
        .as_ref()
        .map(|r| r.business_hours.clone())
        .unwrap_or_else(|| serde_json::json!({}));

    if let Some(tz) = req.timezone {
        validate_timezone(&tz)?;
        timezone = tz.trim().to_string();
    }
    if let Some(sm) = req.default_slot_minutes {
        validate_slot_minutes(sm)?;
        default_slot_minutes = sm;
    }
    if let Some(bh) = req.business_hours {
        validate_business_hours(&bh)?;
        business_hours = bh;
    }

    // IMPORTANT: sqlx::query! params must be passed in the macro call
    let updated = sqlx::query!(
        r#"
        INSERT INTO clinic_settings (
          singleton_id,
          clinic_name,
          timezone,
          default_slot_minutes,
          business_hours,
          updated_at,
          updated_by_user_id
        )
        VALUES (
          TRUE,
          COALESCE((SELECT clinic_name FROM clinic_settings WHERE singleton_id=TRUE), 'Clinic'),
          $1, $2, $3,
          now(),
          $4
        )
        ON CONFLICT (singleton_id)
        DO UPDATE SET
          timezone = EXCLUDED.timezone,
          default_slot_minutes = EXCLUDED.default_slot_minutes,
          business_hours = EXCLUDED.business_hours,
          updated_at = now(),
          updated_by_user_id = EXCLUDED.updated_by_user_id
        RETURNING
          timezone,
          default_slot_minutes,
          business_hours,
          updated_at,
          updated_by_user_id
        "#,
        timezone,             // $1
        default_slot_minutes, // $2
        business_hours,       // $3
        auth.user_id          // $4
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(ClinicSettingsResponse {
        data: ClinicSettingsData {
            timezone: updated.timezone,
            default_slot_minutes: updated.default_slot_minutes,
            business_hours: updated.business_hours,
            updated_at: updated.updated_at.to_rfc3339(),
            updated_by_user_id: updated.updated_by_user_id.map(|u| u.to_string()),
        },
    }))
}

/* ============================================================
   3) /clinic/meta (DERIVED UI HELPER)
   ============================================================ */

#[derive(Debug, Serialize)]
pub struct ClinicMetaResponse {
    pub data: ClinicMetaData,
}

#[derive(Debug, Serialize)]
pub struct ClinicMetaData {
    pub timezone: String,
    pub default_slot_minutes: i32,
    pub business_hours: JsonValue,
    pub slot_options: Vec<i32>,
    pub day_keys: Vec<&'static str>,
}

pub async fn get_clinic_meta(
    State(state): State<AppState>,
    _auth: AuthContext,
) -> Result<Json<ClinicMetaResponse>, ApiError> {
    let row = sqlx::query!(
        r#"
        SELECT timezone, default_slot_minutes, business_hours
        FROM clinic_settings
        WHERE singleton_id = TRUE
        "#
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    let timezone = row
        .as_ref()
        .map(|r| r.timezone.clone())
        .unwrap_or_else(|| "UTC".into());

    let default_slot_minutes = row
        .as_ref()
        .map(|r| r.default_slot_minutes)
        .unwrap_or(30);

    let business_hours = row
        .as_ref()
        .map(|r| r.business_hours.clone())
        .unwrap_or_else(|| serde_json::json!({}));

    // UI helper: let frontend populate dropdown quickly
    let slot_options = vec![5, 10, 15, 20, 30, 45, 60];
    let day_keys = vec!["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

    Ok(Json(ClinicMetaResponse {
        data: ClinicMetaData {
            timezone,
            default_slot_minutes,
            business_hours,
            slot_options,
            day_keys,
        },
    }))
}
