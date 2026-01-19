// src/routes/patient_comm_routes.rs

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::QueryBuilder;
use uuid::Uuid;

use crate::{
    error::ApiError,
    middleware::auth_context::AuthContext,
    models::{AppState, OkData, OkResponse, PhoneNumberRow, SmsDirection, SmsRow},
};

// --------------------------
// Router
// --------------------------

pub fn router() -> Router<AppState> {
    Router::new()
        // -----------------------
        // Phone numbers (per patient)
        // -----------------------
        .route(
            "/patients/{patient_id}/phone_numbers",
            get(list_phone_numbers).post(add_phone_number),
        )
        // Alias: already the same endpoint; kept for clarity / future separation.
        .route(
            "/patients/{patient_id}/phone_numbers_alias",
            get(list_phone_numbers),
        )
        // Phone number utility
        .route("/phone_numbers/normalize", post(normalize_phone_number))
        // Phone number single-resource endpoints
        .route(
            "/phone_numbers/{phone_number_id}",
            get(get_phone_number)
                .patch(update_phone_number)
                .delete(delete_phone_number),
        )
        .route(
            "/phone_numbers/{phone_number_id}/make_primary",
            post(make_primary),
        )
        // -----------------------
        // SMS (per phone number)
        // -----------------------
        .route(
            "/phone_numbers/{phone_number_id}/sms",
            get(list_sms_for_phone).post(add_sms),
        )
        // -----------------------
        // SMS (global)
        // -----------------------
        .route("/sms", get(search_sms))
        .route("/sms/{sms_id}", get(get_sms).delete(delete_sms))
        .route("/sms/bulk_send", post(bulk_send_sms))
        .route("/sms/render", post(render_sms_template))
}

// --------------------------
// RBAC helpers (simple for now)
// --------------------------
// roles: 0 patient, 1 admin, 2 manager, 3 doctor, 4 receptionist

fn ensure_staff(_auth: &AuthContext) -> Result<(), ApiError> {
    // tighten later. For now: any authenticated user can call.
    Ok(())
}

fn ensure_admin(auth: &AuthContext) -> Result<(), ApiError> {
    if auth.role == 1 {
        Ok(())
    } else {
        Err(ApiError::Forbidden("FORBIDDEN", "admin only".into()))
    }
}

fn ensure_admin_or_manager(auth: &AuthContext) -> Result<(), ApiError> {
    if auth.role == 1 || auth.role == 2 {
        Ok(())
    } else {
        Err(ApiError::Forbidden("FORBIDDEN", "admin/manager only".into()))
    }
}

// --------------------------
// Phone numbers: list + add
// --------------------------

pub async fn list_phone_numbers(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(patient_id): Path<Uuid>,
) -> Result<Json<Vec<PhoneNumberRow>>, ApiError> {
    ensure_staff(&auth)?;

    let rows: Vec<PhoneNumberRow> = sqlx::query_as::<_, PhoneNumberRow>(
        r#"
        SELECT
          phone_number_id,
          patient_id,
          phone_number,
          label,
          is_primary,
          created_at,
          updated_at
        FROM phone_number
        WHERE patient_id = $1
        ORDER BY is_primary DESC, created_at ASC
        "#,
    )
    .bind(patient_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct AddPhoneNumberRequest {
    pub phone_number: String,
    pub label: String,
    pub is_primary: Option<bool>,
}

pub async fn add_phone_number(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(patient_id): Path<Uuid>,
    Json(req): Json<AddPhoneNumberRequest>,
) -> Result<Json<PhoneNumberRow>, ApiError> {
    ensure_staff(&auth)?;

    let phone_number = normalize_e164_strict(req.phone_number.trim())?;
    let label = req.label.trim();

    if label.is_empty() {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "label is required".into(),
        ));
    }

    let is_primary = req.is_primary.unwrap_or(false);

    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    if is_primary {
        sqlx::query(
            r#"
            UPDATE phone_number
            SET is_primary = false, updated_at = now()
            WHERE patient_id = $1 AND is_primary = true
            "#,
        )
        .bind(patient_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;
    }

    let row: PhoneNumberRow = sqlx::query_as::<_, PhoneNumberRow>(
        r#"
        INSERT INTO phone_number (patient_id, phone_number, label, is_primary)
        VALUES ($1, $2, $3, $4)
        RETURNING
          phone_number_id,
          patient_id,
          phone_number,
          label,
          is_primary,
          created_at,
          updated_at
        "#,
    )
    .bind(patient_id)
    .bind(&phone_number)
    .bind(label)
    .bind(is_primary)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(row))
}

// --------------------------
// Phone numbers: normalize
// --------------------------

#[derive(Debug, Deserialize)]
pub struct NormalizeRequest {
    pub raw: String,
}

#[derive(Debug, Serialize)]
pub struct NormalizeResponse {
    pub data: NormalizeData,
}

#[derive(Debug, Serialize)]
pub struct NormalizeData {
    pub normalized: String,
}

pub async fn normalize_phone_number(
    State(_state): State<AppState>,
    auth: AuthContext,
    Json(req): Json<NormalizeRequest>,
) -> Result<Json<NormalizeResponse>, ApiError> {
    ensure_staff(&auth)?;

    let normalized = normalize_e164_strict(req.raw.trim())?;
    Ok(Json(NormalizeResponse {
        data: NormalizeData { normalized },
    }))
}

fn normalize_e164_strict(raw: &str) -> Result<String, ApiError> {
    let mut s = raw.trim().to_string();

    s = s.replace(' ', "")
        .replace('-', "")
        .replace('(', "")
        .replace(')', "")
        .replace('.', "");

    // Support "00" prefix
    if s.starts_with("00") {
        s = format!("+{}", &s[2..]);
    }

    if !s.starts_with('+') {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "phone number must start with + (E.164), e.g. +8613812345678".into(),
        ));
    }

    let digits = &s[1..];
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "phone number must contain only digits after +".into(),
        ));
    }

    if digits.len() > 15 {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "phone number too long for E.164 (max 15 digits)".into(),
        ));
    }

    Ok(s)
}

// --------------------------
// Phone numbers: GET one
// --------------------------

pub async fn get_phone_number(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(phone_number_id): Path<Uuid>,
) -> Result<Json<PhoneNumberRow>, ApiError> {
    ensure_staff(&auth)?;

    let row: PhoneNumberRow = sqlx::query_as::<_, PhoneNumberRow>(
        r#"
        SELECT
          phone_number_id,
          patient_id,
          phone_number,
          label,
          is_primary,
          created_at,
          updated_at
        FROM phone_number
        WHERE phone_number_id = $1
        "#,
    )
    .bind(phone_number_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    .ok_or_else(|| ApiError::BadRequest("NOT_FOUND", "phone number not found".into()))?;

    Ok(Json(row))
}

// --------------------------
// Phone numbers: make_primary
// --------------------------

pub async fn make_primary(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(phone_number_id): Path<Uuid>,
) -> Result<Json<PhoneNumberRow>, ApiError> {
    ensure_staff(&auth)?;

    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    let patient_id: Uuid = sqlx::query_scalar(
        r#"
        SELECT patient_id
        FROM phone_number
        WHERE phone_number_id = $1
        "#,
    )
    .bind(phone_number_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    .ok_or_else(|| ApiError::BadRequest("NOT_FOUND", "phone number not found".into()))?;

    // unset all for patient
    sqlx::query(
        r#"
        UPDATE phone_number
        SET is_primary = FALSE, updated_at = now()
        WHERE patient_id = $1
        "#,
    )
    .bind(patient_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    // set this one
    let updated: PhoneNumberRow = sqlx::query_as::<_, PhoneNumberRow>(
        r#"
        UPDATE phone_number
        SET is_primary = TRUE, updated_at = now()
        WHERE phone_number_id = $1
        RETURNING
          phone_number_id,
          patient_id,
          phone_number,
          label,
          is_primary,
          created_at,
          updated_at
        "#,
    )
    .bind(phone_number_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(updated))
}

// --------------------------
// Phone numbers: PATCH
// --------------------------

#[derive(Debug, Deserialize)]
pub struct UpdatePhoneNumberRequest {
    pub phone_number: Option<String>,
    pub label: Option<String>,
    pub is_primary: Option<bool>,
}

pub async fn update_phone_number(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(phone_number_id): Path<Uuid>,
    Json(req): Json<UpdatePhoneNumberRequest>,
) -> Result<Json<PhoneNumberRow>, ApiError> {
    ensure_staff(&auth)?;

    let existing: PhoneNumberRow = sqlx::query_as::<_, PhoneNumberRow>(
        r#"
        SELECT
          phone_number_id,
          patient_id,
          phone_number,
          label,
          is_primary,
          created_at,
          updated_at
        FROM phone_number
        WHERE phone_number_id = $1
        "#,
    )
    .bind(phone_number_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    .ok_or_else(|| ApiError::BadRequest("NOT_FOUND", "phone number not found".into()))?;

    let new_phone = match req.phone_number.as_deref().map(str::trim) {
        Some(s) if !s.is_empty() => normalize_e164_strict(s)?,
        _ => existing.phone_number.clone(),
    };

    let new_label: String = match req.label.as_deref().map(str::trim) {
        None => existing.label.clone(),
        Some(v) if v.is_empty() => {
            return Err(ApiError::BadRequest(
                "VALIDATION_ERROR",
                "label cannot be empty".into(),
            ))
        }
        Some(v) => v.to_string(),
    };

    let want_primary = req.is_primary.unwrap_or(existing.is_primary);

    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    // update base fields
    sqlx::query(
        r#"
        UPDATE phone_number
        SET phone_number = $1,
            label = $2,
            updated_at = now()
        WHERE phone_number_id = $3
        "#,
    )
    .bind(&new_phone)
    .bind(&new_label)
    .bind(phone_number_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    // enforce one primary
    if want_primary {
        sqlx::query(
            r#"
            UPDATE phone_number
            SET is_primary = FALSE, updated_at = now()
            WHERE patient_id = $1
            "#,
        )
        .bind(existing.patient_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

        sqlx::query(
            r#"
            UPDATE phone_number
            SET is_primary = TRUE, updated_at = now()
            WHERE phone_number_id = $1
            "#,
        )
        .bind(phone_number_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;
    } else if req.is_primary == Some(false) {
        sqlx::query(
            r#"
            UPDATE phone_number
            SET is_primary = FALSE, updated_at = now()
            WHERE phone_number_id = $1
            "#,
        )
        .bind(phone_number_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;
    }

    let out: PhoneNumberRow = sqlx::query_as::<_, PhoneNumberRow>(
        r#"
        SELECT
          phone_number_id,
          patient_id,
          phone_number,
          label,
          is_primary,
          created_at,
          updated_at
        FROM phone_number
        WHERE phone_number_id = $1
        "#,
    )
    .bind(phone_number_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(out))
}

// --------------------------
// Phone numbers: DELETE (admin/manager only)
// - blocked if SMS exists
// --------------------------

pub async fn delete_phone_number(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(phone_number_id): Path<Uuid>,
) -> Result<Json<OkResponse>, ApiError> {
    ensure_admin_or_manager(&auth)?;

    // Use EXISTS -> bool to avoid scalar type decoding mismatches (prevents 500s)
    let has_sms: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM sms
            WHERE phone_number_id = $1
        )
        "#,
    )
    .bind(phone_number_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    if has_sms {
        return Err(ApiError::BadRequest(
            "CONFLICT",
            "Cannot delete phone number: it has SMS history.".into(),
        ));
    }

    let res = sqlx::query(
        r#"
        DELETE FROM phone_number
        WHERE phone_number_id = $1
        "#,
    )
    .bind(phone_number_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    if res.rows_affected() == 0 {
        return Err(ApiError::BadRequest("NOT_FOUND", "phone number not found".into()));
    }

    Ok(Json(OkResponse {
        data: OkData { ok: true },
    }))
}

// ============================================================================
// SMS (per phone_number): create + list
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct AddSmsRequest {
    pub direction: i16, // 0=in, 1=out
    pub sent_at: Option<DateTime<Utc>>,
    pub subject: Option<String>,
    pub sms_text: String,
    pub note: Option<String>,
}

pub async fn add_sms(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(phone_number_id): Path<Uuid>,
    Json(req): Json<AddSmsRequest>,
) -> Result<Json<SmsRow>, ApiError> {
    ensure_staff(&auth)?;

    if req.direction != 0 && req.direction != 1 {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "direction must be 0 or 1".into(),
        ));
    }

    let sms_text = req.sms_text.trim();
    if sms_text.is_empty() {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "sms_text is required".into(),
        ));
    }

    let sent_at = req.sent_at.unwrap_or_else(Utc::now);

    let row: SmsRow = sqlx::query_as::<_, SmsRow>(
        r#"
        INSERT INTO sms (phone_number_id, direction, sent_at, subject, sms_text, note)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING
          sms_id,
          phone_number_id,
          direction,
          sent_at,
          subject,
          sms_text,
          note,
          created_at
        "#,
    )
    .bind(phone_number_id)
    .bind(req.direction)
    .bind(sent_at)
    .bind(req.subject.as_deref())
    .bind(sms_text)
    .bind(req.note.as_deref())
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(row))
}

pub async fn list_sms_for_phone(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(phone_number_id): Path<Uuid>,
) -> Result<Json<Vec<SmsRow>>, ApiError> {
    ensure_staff(&auth)?;

    let rows: Vec<SmsRow> = sqlx::query_as::<_, SmsRow>(
        r#"
        SELECT
          sms_id,
          phone_number_id,
          direction,
          sent_at,
          subject,
          sms_text,
          note,
          created_at
        FROM sms
        WHERE phone_number_id = $1
        ORDER BY sent_at DESC
        "#,
    )
    .bind(phone_number_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(rows))
}

// ============================================================================
// SMS (global): GET one, search, delete
// ============================================================================

pub async fn get_sms(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(sms_id): Path<Uuid>,
) -> Result<Json<SmsRow>, ApiError> {
    ensure_staff(&auth)?;

    let row: SmsRow = sqlx::query_as::<_, SmsRow>(
        r#"
        SELECT
          sms_id,
          phone_number_id,
          direction,
          sent_at,
          subject,
          sms_text,
          note,
          created_at
        FROM sms
        WHERE sms_id = $1
        "#,
    )
    .bind(sms_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    .ok_or_else(|| ApiError::BadRequest("NOT_FOUND", "sms not found".into()))?;

    Ok(Json(row))
}

#[derive(Debug, Deserialize)]
pub struct SmsSearchQuery {
    pub patient_id: Option<Uuid>,
    pub phone_number_id: Option<Uuid>,
    pub direction: Option<i16>, // 0 or 1
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub q: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn search_sms(
    State(state): State<AppState>,
    auth: AuthContext,
    Query(q): Query<SmsSearchQuery>,
) -> Result<Json<Vec<SmsRow>>, ApiError> {
    ensure_staff(&auth)?;

    if let Some(d) = q.direction {
        if d != 0 && d != 1 {
            return Err(ApiError::BadRequest(
                "VALIDATION_ERROR",
                "direction must be 0 or 1".into(),
            ));
        }
    }

    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).max(0);

    // Use QueryBuilder for safe dynamic SQL
    let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
        r#"
        SELECT
          s.sms_id,
          s.phone_number_id,
          s.direction,
          s.sent_at,
          s.subject,
          s.sms_text,
          s.note,
          s.created_at
        FROM sms s
        "#,
    );

    // join only if patient_id filtering is used
    if q.patient_id.is_some() {
        qb.push(" JOIN phone_number pn ON pn.phone_number_id = s.phone_number_id ");
    }

    qb.push(" WHERE 1=1 ");

    if let Some(pid) = q.patient_id {
        qb.push(" AND pn.patient_id = ");
        qb.push_bind(pid);
    }
    if let Some(pnid) = q.phone_number_id {
        qb.push(" AND s.phone_number_id = ");
        qb.push_bind(pnid);
    }
    if let Some(dir) = q.direction {
        qb.push(" AND s.direction = ");
        qb.push_bind(dir);
    }
    if let Some(from) = q.from {
        qb.push(" AND s.sent_at >= ");
        qb.push_bind(from);
    }
    if let Some(to) = q.to {
        qb.push(" AND s.sent_at <= ");
        qb.push_bind(to);
    }
    if let Some(keyword) = q.q.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let like = format!("%{}%", keyword);
    
        qb.push(" AND (s.sms_text ILIKE ");
        qb.push_bind(like.clone());   // bind owned
        qb.push(" OR s.subject ILIKE ");
        qb.push_bind(like);           // move owned
        qb.push(") ");
    }
    

    qb.push(" ORDER BY s.sent_at DESC ");
    qb.push(" LIMIT ");
    qb.push_bind(limit);
    qb.push(" OFFSET ");
    qb.push_bind(offset);

    let rows: Vec<SmsRow> = qb
        .build_query_as::<SmsRow>()
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(rows))
}

pub async fn delete_sms(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(sms_id): Path<Uuid>,
) -> Result<Json<OkResponse>, ApiError> {
    // Spec: admin-only delete
    ensure_admin(&auth)?;

    let res = sqlx::query(
        r#"
        DELETE FROM sms
        WHERE sms_id = $1
        "#,
    )
    .bind(sms_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    if res.rows_affected() == 0 {
        return Err(ApiError::BadRequest("NOT_FOUND", "sms not found".into()));
    }

    Ok(Json(OkResponse {
        data: OkData { ok: true },
    }))
}

// ============================================================================
// SMS bulk_send: store rows only (direction=Send)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct BulkSendRequest {
    pub phone_number_ids: Vec<Uuid>,
    pub text: String,
    pub dry_run: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct BulkSendResponse {
    pub data: BulkSendData,
}

#[derive(Debug, Serialize)]
pub struct BulkSendData {
    pub dry_run: bool,
    pub requested: usize,
    pub valid: usize,
    pub created: usize,
    pub invalid_phone_number_ids: Vec<Uuid>,
    pub sms_rows: Vec<SmsRow>,
}

pub async fn bulk_send_sms(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(req): Json<BulkSendRequest>,
) -> Result<Json<BulkSendResponse>, ApiError> {
    ensure_staff(&auth)?;

    let dry_run = req.dry_run.unwrap_or(false);
    let text = req.text.trim();
    if text.is_empty() {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "text is required".into(),
        ));
    }
    if req.phone_number_ids.is_empty() {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "phone_number_ids cannot be empty".into(),
        ));
    }

    // Cap for safety
    if req.phone_number_ids.len() > 500 {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "too many recipients (max 500)".into(),
        ));
    }

    // Validate IDs exist
    let existing_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT phone_number_id
        FROM phone_number
        WHERE phone_number_id = ANY($1)
        "#,
    )
    .bind(&req.phone_number_ids)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    let mut invalid = Vec::new();
    for id in &req.phone_number_ids {
        if !existing_ids.contains(id) {
            invalid.push(*id);
        }
    }

    let valid_ids = existing_ids;
    let valid_count = valid_ids.len();

    if dry_run {
        return Ok(Json(BulkSendResponse {
            data: BulkSendData {
                dry_run: true,
                requested: req.phone_number_ids.len(),
                valid: valid_count,
                created: 0,
                invalid_phone_number_ids: invalid,
                sms_rows: vec![],
            },
        }));
    }

    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    let mut created_rows: Vec<SmsRow> = Vec::with_capacity(valid_count);

    // Insert one row per recipient
    for pnid in valid_ids {
        let row: SmsRow = sqlx::query_as::<_, SmsRow>(
            r#"
            INSERT INTO sms (phone_number_id, direction, sent_at, subject, sms_text, note)
            VALUES ($1, $2, now(), NULL, $3, NULL)
            RETURNING
              sms_id,
              phone_number_id,
              direction,
              sent_at,
              subject,
              sms_text,
              note,
              created_at
            "#,
        )
        .bind(pnid)
        .bind(SmsDirection::Send as i16)
        .bind(text)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

        created_rows.push(row);
    }

    tx.commit()
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(BulkSendResponse {
        data: BulkSendData {
            dry_run: false,
            requested: req.phone_number_ids.len(),
            valid: valid_count,
            created: created_rows.len(),
            invalid_phone_number_ids: invalid,
            sms_rows: created_rows,
        },
    }))
}

// ============================================================================
// SMS render: simple placeholder replacement (no schema change)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct RenderTemplateRequest {
    pub template: String,
    pub patient_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct RenderTemplateResponse {
    pub data: RenderTemplateData,
}

#[derive(Debug, Serialize)]
pub struct RenderTemplateData {
    pub rendered: String,
}

#[derive(Debug, sqlx::FromRow)]
struct PatientLiteRow {
    register_number: String,
    first_name: String,
    last_name: String,
}

pub async fn render_sms_template(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(req): Json<RenderTemplateRequest>,
) -> Result<Json<RenderTemplateResponse>, ApiError> {
    ensure_staff(&auth)?;

    let tpl = req.template.trim().to_string();
    if tpl.is_empty() {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "template is required".into(),
        ));
    }

    let p: PatientLiteRow = sqlx::query_as::<_, PatientLiteRow>(
        r#"
        SELECT register_number, first_name, last_name
        FROM patient
        WHERE patient_id = $1
        "#,
    )
    .bind(req.patient_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    .ok_or_else(|| ApiError::BadRequest("NOT_FOUND", "patient not found".into()))?;

    let full_name = format!("{} {}", p.first_name, p.last_name);

    // Simple placeholders you can expand later:
    // {name}, {first_name}, {last_name}, {register_number}
    let rendered = tpl
        .replace("{name}", &full_name)
        .replace("{first_name}", &p.first_name)
        .replace("{last_name}", &p.last_name)
        .replace("{register_number}", &p.register_number);

    Ok(Json(RenderTemplateResponse {
        data: RenderTemplateData { rendered },
    }))
}
