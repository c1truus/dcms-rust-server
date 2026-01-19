// src/routes/patient_routes.rs

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::ApiError,
    middleware::auth_context::AuthContext,
    models::AppState,
};

// use axum::routing::patch;
// use serde_json::json;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PatientRow {
    pub patient_id: Uuid,
    pub register_number: String,
    pub user_id: Option<Uuid>,
    pub first_name: String,
    pub last_name: String,
    pub email: Option<String>,
    pub birthday: Option<chrono::NaiveDate>,
    pub gender: i16,
    pub status: i16,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePatientRequest {
    pub register_number: Option<String>, // allow override, otherwise DB default generates it
    pub first_name: String,
    pub last_name: String,
    pub email: Option<String>,
    pub birthday: Option<chrono::NaiveDate>,
    pub gender: i16, // 0,1,2
    pub status: Option<i16>, // default 0
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/patients", post(create_patient).get(search_patients))
        .route("/patients/{patient_id}", get(get_patient).patch(update_patient))
        .route("/patients/{patient_id}/summary", get(get_patient_summary))
        .route("/patients/{patient_id}/archive", post(archive_patient))
        .route("/patients/{patient_id}/restore", post(restore_patient))
        .route("/patients/{patient_id}/link_user/{user_id}", post(link_patient_user))
        .route("/patients/{patient_id}/unlink_user", post(unlink_patient_user))
}

use serde::de::Deserializer;

fn deserialize_double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    // This is called only when the field is present (even if it's `null`).
    // - null => Option::<T>::deserialize => None => we wrap => Some(None)
    // - value => Some(value) => we wrap => Some(Some(value))
    let inner = Option::<T>::deserialize(deserializer)?;
    Ok(Some(inner))
}


fn ensure_staff(auth: &AuthContext) -> Result<(), ApiError> {
    // adjust to your role model; currently you return Vec<String> roles in /me
    // Here, AuthContext likely has role(s) derived from dcms_user.roles smallint.
    // We'll assume it can be checked via helper method you already use.
    //
    // Minimal: allow all authenticated users for now.
    let _ = auth;
    Ok(())
}

pub async fn create_patient(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(req): Json<CreatePatientRequest>,
) -> Result<Json<PatientRow>, ApiError> {
    ensure_staff(&auth)?;

    let first_name = req.first_name.trim();
    let last_name = req.last_name.trim();

    if first_name.is_empty() || last_name.is_empty() {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "first_name and last_name are required".to_string(),
        ));
    }
    if req.gender < 0 || req.gender > 2 {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "gender must be 0,1,2".to_string(),
        ));
    }

    let status = req.status.unwrap_or(0);

    // If register_number provided, insert it; else rely on DB default
    let row: PatientRow = if let Some(rn) = req.register_number.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        sqlx::query_as::<_, PatientRow>(
            r#"
            INSERT INTO patient (register_number, first_name, last_name, email, birthday, gender, status, created_at, last_seen_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7, now(), now())
            RETURNING patient_id, register_number, user_id, first_name, last_name, email, birthday, gender, status, created_at, last_seen_at
            "#,
        )
        .bind(rn)
        .bind(first_name)
        .bind(last_name)
        .bind(req.email.as_deref())
        .bind(req.birthday)
        .bind(req.gender)
        .bind(status)
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    } else {
        sqlx::query_as::<_, PatientRow>(
            r#"
            INSERT INTO patient (first_name, last_name, email, birthday, gender, status, created_at, last_seen_at)
            VALUES ($1,$2,$3,$4,$5,$6, now(), now())
            RETURNING patient_id, register_number, user_id, first_name, last_name, email, birthday, gender, status, created_at, last_seen_at
            "#,
        )
        .bind(first_name)
        .bind(last_name)
        .bind(req.email.as_deref())
        .bind(req.birthday)
        .bind(req.gender)
        .bind(status)
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    };

    Ok(Json(row))
}

pub async fn get_patient(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(patient_id): Path<Uuid>,
) -> Result<Json<PatientRow>, ApiError> {
    ensure_staff(&auth)?;

    let row: PatientRow = sqlx::query_as::<_, PatientRow>(
        r#"
        SELECT patient_id, register_number, user_id, first_name, last_name, email, birthday, gender, status, created_at, last_seen_at
        FROM patient
        WHERE patient_id = $1
        "#,
    )
    .bind(patient_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    .ok_or_else(|| ApiError::BadRequest("NOT_FOUND", "patient not found".to_string()))?;

    Ok(Json(row))
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub query: Option<String>,
}

pub async fn search_patients(
    State(state): State<AppState>,
    auth: AuthContext,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Vec<PatientRow>>, ApiError> {
    ensure_staff(&auth)?;

    let query = q.query.unwrap_or_default().trim().to_string();
    if query.is_empty() {
        // default: most recent
        let rows: Vec<PatientRow> = sqlx::query_as::<_, PatientRow>(
            r#"
            SELECT patient_id, register_number, user_id, first_name, last_name, email, birthday, gender, status, created_at, last_seen_at
            FROM patient
            ORDER BY created_at DESC
            LIMIT 50
            "#,
        )
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;
        return Ok(Json(rows));
    }

    let like = format!("%{}%", query);

    let rows: Vec<PatientRow> = sqlx::query_as::<_, PatientRow>(
        r#"
        SELECT patient_id, register_number, user_id, first_name, last_name, email, birthday, gender, status, created_at, last_seen_at
        FROM patient
        WHERE register_number ILIKE $1
           OR first_name ILIKE $1
           OR last_name ILIKE $1
        ORDER BY created_at DESC
        LIMIT 50
        "#,
    )
    .bind(like)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct UpdatePatientRequest {
    pub register_number: Option<String>, // optional override (rare; usually keep stable)
    pub user_id: Option<Uuid>,           // allow linking in PATCH (optional)
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    // pub email: Option<Option<String>>,      // set to null: send null? => we handle via Option<Option<String>> below if needed
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub email: Option<Option<String>>,
    pub birthday: Option<chrono::NaiveDate>,
    pub gender: Option<i16>,
    pub status: Option<i16>,
}

pub async fn update_patient(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(patient_id): Path<Uuid>,
    Json(req): Json<UpdatePatientRequest>,
) -> Result<Json<PatientRow>, ApiError> {
    ensure_staff(&auth)?;

    // Load existing
    let existing: PatientRow = sqlx::query_as::<_, PatientRow>(
        r#"
        SELECT patient_id, register_number, user_id, first_name, last_name, email,
               birthday, gender, status, created_at, last_seen_at
        FROM patient
        WHERE patient_id = $1
        "#,
    )
    .bind(patient_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    .ok_or_else(|| ApiError::BadRequest("NOT_FOUND", "patient not found".to_string()))?;

    // Apply updates with validation
    let register_number = match req.register_number.as_deref().map(str::trim) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => existing.register_number.clone(),
    };

    let first_name = match req.first_name.as_deref().map(str::trim) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => existing.first_name.clone(),
    };

    let last_name = match req.last_name.as_deref().map(str::trim) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => existing.last_name.clone(),
    };

    // For email: if provided as Some(""), treat as clearing
    let email: Option<String> = match req.email {
        None => existing.email.clone(),        // field not provided => keep old
        Some(None) => None,                    // explicitly null => clear
        Some(Some(e)) => {
            let t = e.trim();
            if t.is_empty() { None } else { Some(t.to_string()) }
        }
    };
    

    let birthday = req.birthday.or(existing.birthday);
    let gender = req.gender.unwrap_or(existing.gender);
    let status = req.status.unwrap_or(existing.status);
    let user_id = req.user_id.or(existing.user_id);

    if gender < 0 || gender > 2 {
        return Err(ApiError::BadRequest("VALIDATION_ERROR", "gender must be 0,1,2".into()));
    }
    // status check based on migration: patient.status 0..3
    if status < 0 || status > 3 {
        return Err(ApiError::BadRequest("VALIDATION_ERROR", "status must be 0..3".into()));
    }

    let updated: PatientRow = sqlx::query_as::<_, PatientRow>(
        r#"
        UPDATE patient
        SET register_number = $1,
            user_id = $2,
            first_name = $3,
            last_name = $4,
            email = $5,
            birthday = $6,
            gender = $7,
            status = $8,
            last_seen_at = now()
        WHERE patient_id = $9
        RETURNING patient_id, register_number, user_id, first_name, last_name, email,
                  birthday, gender, status, created_at, last_seen_at
        "#,
    )
    .bind(register_number)
    .bind(user_id)
    .bind(first_name)
    .bind(last_name)
    .bind(email)
    .bind(birthday)
    .bind(gender)
    .bind(status)
    .bind(patient_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(updated))
}

pub async fn link_patient_user(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((patient_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<PatientRow>, ApiError> {
    ensure_staff(&auth)?;

    // Ensure target user exists
    let exists: Option<Uuid> = sqlx::query_scalar(
        r#"
        SELECT user_id
        FROM "dcms_user"
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    if exists.is_none() {
        return Err(ApiError::BadRequest("NOT_FOUND", "user not found".into()));
    }

    let updated: PatientRow = sqlx::query_as::<_, PatientRow>(
        r#"
        UPDATE patient
        SET user_id = $1, last_seen_at = now()
        WHERE patient_id = $2
        RETURNING patient_id, register_number, user_id, first_name, last_name, email,
                  birthday, gender, status, created_at, last_seen_at
        "#,
    )
    .bind(user_id)
    .bind(patient_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    .ok_or_else(|| ApiError::BadRequest("NOT_FOUND", "patient not found".into()))?;

    Ok(Json(updated))
}

pub async fn unlink_patient_user(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(patient_id): Path<Uuid>,
) -> Result<Json<PatientRow>, ApiError> {
    ensure_staff(&auth)?;

    let updated: PatientRow = sqlx::query_as::<_, PatientRow>(
        r#"
        UPDATE patient
        SET user_id = NULL, last_seen_at = now()
        WHERE patient_id = $1
        RETURNING patient_id, register_number, user_id, first_name, last_name, email,
                  birthday, gender, status, created_at, last_seen_at
        "#,
    )
    .bind(patient_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    .ok_or_else(|| ApiError::BadRequest("NOT_FOUND", "patient not found".into()))?;

    Ok(Json(updated))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PhoneNumberRow {
    pub phone_number_id: Uuid,
    pub patient_id: Uuid,
    pub phone_number: String,
    pub label: Option<String>,
    pub is_primary: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SmsRow {
    pub sms_id: Uuid,
    pub phone_number_id: Uuid,
    pub direction: i16,
    pub sent_at: chrono::DateTime<chrono::Utc>,
    pub sms_text: String,
}

#[derive(Debug, Serialize)]
pub struct PatientSummaryResponse {
    pub data: PatientSummaryData,
}

#[derive(Debug, Serialize)]
pub struct PatientSummaryData {
    pub patient: PatientRow,
    pub phone_numbers: Vec<PhoneNumberRow>,
    pub recent_sms: Vec<SmsRow>,
}

pub async fn get_patient_summary(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(patient_id): Path<Uuid>,
) -> Result<Json<PatientSummaryResponse>, ApiError> {
    ensure_staff(&auth)?;

    // patient
    let patient: PatientRow = sqlx::query_as::<_, PatientRow>(
        r#"
        SELECT patient_id, register_number, user_id, first_name, last_name, email,
               birthday, gender, status, created_at, last_seen_at
        FROM patient
        WHERE patient_id = $1
        "#,
    )
    .bind(patient_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    .ok_or_else(|| ApiError::BadRequest("NOT_FOUND", "patient not found".into()))?;

    // phone numbers
    let phone_numbers: Vec<PhoneNumberRow> = sqlx::query_as::<_, PhoneNumberRow>(
        r#"
        SELECT phone_number_id, patient_id, phone_number, label, is_primary, created_at
        FROM phone_number
        WHERE patient_id = $1
        ORDER BY is_primary DESC, created_at DESC
        "#,
    )
    .bind(patient_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    // recent sms across those phone numbers
    let recent_sms: Vec<SmsRow> = sqlx::query_as::<_, SmsRow>(
        r#"
        SELECT s.sms_id, s.phone_number_id, s.direction, s.sent_at, s.sms_text
        FROM sms s
        JOIN phone_number pn ON pn.phone_number_id = s.phone_number_id
        WHERE pn.patient_id = $1
        ORDER BY s.sent_at DESC
        LIMIT 30
        "#,
    )
    .bind(patient_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(PatientSummaryResponse {
        data: PatientSummaryData {
            patient,
            phone_numbers,
            recent_sms,
        },
    }))
}

const PATIENT_STATUS_ACTIVE: i16 = 0;
const PATIENT_STATUS_ARCHIVED: i16 = 3;

pub async fn archive_patient(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(patient_id): Path<Uuid>,
) -> Result<Json<PatientRow>, ApiError> {
    ensure_staff(&auth)?;

    let updated: PatientRow = sqlx::query_as::<_, PatientRow>(
        r#"
        UPDATE patient
        SET status = $1, last_seen_at = now()
        WHERE patient_id = $2
        RETURNING patient_id, register_number, user_id, first_name, last_name, email,
                  birthday, gender, status, created_at, last_seen_at
        "#,
    )
    .bind(PATIENT_STATUS_ARCHIVED)
    .bind(patient_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    .ok_or_else(|| ApiError::BadRequest("NOT_FOUND", "patient not found".into()))?;

    Ok(Json(updated))
}

pub async fn restore_patient(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(patient_id): Path<Uuid>,
) -> Result<Json<PatientRow>, ApiError> {
    ensure_staff(&auth)?;

    let updated: PatientRow = sqlx::query_as::<_, PatientRow>(
        r#"
        UPDATE patient
        SET status = $1, last_seen_at = now()
        WHERE patient_id = $2
        RETURNING patient_id, register_number, user_id, first_name, last_name, email,
                  birthday, gender, status, created_at, last_seen_at
        "#,
    )
    .bind(PATIENT_STATUS_ACTIVE)
    .bind(patient_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    .ok_or_else(|| ApiError::BadRequest("NOT_FOUND", "patient not found".into()))?;

    Ok(Json(updated))
}
