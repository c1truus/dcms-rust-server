// src/routes/appointment_routes.rs

use axum::{
    extract::{Path, Query, State},
    routing::{get, patch, post, put},
    Json, Router,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
// use serde_json::Value as JsonValue;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    error::ApiError,
    middleware::auth_context::AuthContext,
    models::AppState,
};

/*
Roles (dcms_user.roles):
0 patient
1 admin
2 manager
3 doctor
4 receptionist
*/

fn is_admin(auth: &AuthContext) -> bool {
    auth.role == 1
}
fn is_manager(auth: &AuthContext) -> bool {
    auth.role == 2
}
fn is_doctor(auth: &AuthContext) -> bool {
    auth.role == 3
}
fn is_receptionist(auth: &AuthContext) -> bool {
    auth.role == 4
}

fn can_manage_appointments(auth: &AuthContext) -> bool {
    is_admin(auth) || is_manager(auth) || is_receptionist(auth)
}

fn ensure_manage(auth: &AuthContext) -> Result<(), ApiError> {
    if can_manage_appointments(auth) {
        Ok(())
    } else {
        Err(ApiError::Forbidden(
            "FORBIDDEN",
            "Only admin/manager/receptionist can manage appointments".into(),
        ))
    }
}

fn ensure_view_doctor_scope(auth: &AuthContext, requested_doctor: Option<Uuid>) -> Result<Option<Uuid>, ApiError> {
    // If a doctor requests schedule without specifying doctor_employee_id:
    // we resolve it from their linked employee record.
    // If they specify another doctor id, forbid.
    //
    // If admin/manager/receptionist: allow any doctor id (or none = all, but we won’t support “all” in week view).
    if can_manage_appointments(auth) {
        return Ok(requested_doctor);
    }

    if is_doctor(auth) {
        if requested_doctor.is_some() {
            // doctors may only view themselves
            return Err(ApiError::Forbidden(
                "FORBIDDEN",
                "Doctor can only view their own schedule".into(),
            ));
        }
        // doctor with None -> caller will look up employee_id by auth.user_id
        return Ok(None);
    }

    Err(ApiError::Forbidden(
        "FORBIDDEN",
        "You do not have permission to view schedules".into(),
    ))
}

async fn resolve_doctor_employee_id_by_user_id(
    state: &AppState,
    user_id: Uuid,
) -> Result<Uuid, ApiError> {
    let row = sqlx::query(
        r#"
        SELECT employee_id
        FROM employee
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    let Some(row) = row else {
        return Err(ApiError::BadRequest(
            "NO_EMPLOYEE_PROFILE",
            "Doctor account has no employee profile".into(),
        ));
    };

    let employee_id: Uuid = row
        .try_get("employee_id")
        .map_err(|e| ApiError::Internal(format!("row decode error: {e}")))?;
    Ok(employee_id)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/appointments/week", get(get_appointments_week))
        .route("/appointments/today", get(get_appointments_today))
        .route("/appointments/{appointment_id}", get(get_appointment))
        .route("/appointments", post(create_appointment))
        .route("/appointments/{appointment_id}", patch(patch_appointment))
        .route("/appointments/{appointment_id}/arrive", post(mark_arrived))
        .route("/appointments/{appointment_id}/seat", post(mark_seated))
        .route("/appointments/{appointment_id}/dismiss", post(mark_dismissed))
        .route("/appointments/{appointment_id}/plan_items", put(put_plan_items))
}

/* ============================================================
   Response DTOs
   ============================================================ */

#[derive(Debug, Serialize)]
pub struct ApiOk<T> {
    pub data: T,
}

#[derive(Debug, Serialize)]
pub struct PersonBrief {
    pub id: Uuid,
    pub display: String,
    pub number: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct AppointmentPlanItemDto {
    pub service_id: Uuid,
    pub display_name: String,
    pub qty: i32,
}

#[derive(Debug, Serialize)]
pub struct AppointmentBlockDto {
    pub appointment_id: Uuid,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub status: i16,
    pub priority: i16,
    pub color_override: Option<i32>,
    pub note: Option<String>,
    pub patient: PersonBrief,
    pub doctor: PersonBrief,
    pub planned_items: Vec<AppointmentPlanItemDto>,
    pub planned_summary: String,
}

/* ============================================================
   Query params
   ============================================================ */

#[derive(Debug, Deserialize)]
pub struct WeekQuery {
    // YYYY-MM-DD (local interpretation belongs to frontend; DB stores timestamptz)
    pub start: String,
    pub days: Option<i64>,
    pub doctor_employee_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct TodayQuery {
    pub doctor_employee_id: Option<Uuid>,
}

/* ============================================================
   GET /appointments/week
   ============================================================ */

pub async fn get_appointments_week(
    State(state): State<AppState>,
    auth: AuthContext,
    Query(q): Query<WeekQuery>,
) -> Result<Json<ApiOk<Vec<AppointmentBlockDto>>>, ApiError> {
    let days = q.days.unwrap_or(7);
    if !(1..=14).contains(&days) {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "days must be between 1 and 14".into(),
        ));
    }

    let start_date = NaiveDate::parse_from_str(q.start.trim(), "%Y-%m-%d").map_err(|_| {
        ApiError::BadRequest("VALIDATION_ERROR", "start must be YYYY-MM-DD".into())
    })?;

    let requested = ensure_view_doctor_scope(&auth, q.doctor_employee_id)?;

    let doctor_employee_id = match requested {
        Some(id) => id,
        None => {
            // doctor self
            if is_doctor(&auth) {
                resolve_doctor_employee_id_by_user_id(&state, auth.user_id).await?
            } else {
                // If admin/manager/receptionist omitted doctor id, we force them to provide one
                return Err(ApiError::BadRequest(
                    "VALIDATION_ERROR",
                    "doctor_employee_id is required for non-doctor users".into(),
                ));
            }
        }
    };

    // Range: [start, start+days)
    let start_ts = DateTime::<Utc>::from_naive_utc_and_offset(start_date.and_hms_opt(0, 0, 0).unwrap(), Utc);
    let end_ts = start_ts + chrono::Duration::days(days);

    let rows = sqlx::query(
        r#"
        SELECT
          a.appointment_id,
          a.start_at,
          a.end_at,
          a.status,
          a.priority,
          a.color_override,
          a.note,

          p.patient_id,
          p.first_name AS p_first,
          p.last_name  AS p_last,
          p.register_number AS p_reg,

          d.employee_id AS d_id,
          d.employee_display_number AS d_no,
          d.first_name AS d_first,
          d.last_name  AS d_last,

          api.service_id AS svc_id,
          api.qty AS svc_qty,
          sc.display_name AS svc_name,
          sc.display_number AS svc_no

        FROM appointment a
        JOIN patient p ON p.patient_id = a.patient_id
        JOIN employee d ON d.employee_id = a.doctor_employee_id
        LEFT JOIN appointment_plan_item api ON api.appointment_id = a.appointment_id
        LEFT JOIN service_catalog sc ON sc.service_id = api.service_id

        WHERE a.doctor_employee_id = $1
          AND a.start_at >= $2
          AND a.start_at <  $3

        ORDER BY a.start_at ASC, sc.display_number ASC
        "#,
    )
    .bind(doctor_employee_id)
    .bind(start_ts)
    .bind(end_ts)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(ApiOk {
        data: fold_rows_into_blocks(rows)?,
    }))
}

/* ============================================================
   GET /appointments/today
   ============================================================ */

pub async fn get_appointments_today(
    State(state): State<AppState>,
    auth: AuthContext,
    Query(q): Query<TodayQuery>,
) -> Result<Json<ApiOk<Vec<AppointmentBlockDto>>>, ApiError> {
    let requested = ensure_view_doctor_scope(&auth, q.doctor_employee_id)?;

    let doctor_employee_id = match requested {
        Some(id) => id,
        None => {
            if is_doctor(&auth) {
                resolve_doctor_employee_id_by_user_id(&state, auth.user_id).await?
            } else {
                return Err(ApiError::BadRequest(
                    "VALIDATION_ERROR",
                    "doctor_employee_id is required for non-doctor users".into(),
                ));
            }
        }
    };

    let start = chrono::Utc::now().date_naive().and_hms_opt(0, 0, 0).unwrap();
    let start_ts = DateTime::<Utc>::from_naive_utc_and_offset(start, Utc);
    let end_ts = start_ts + chrono::Duration::days(1);

    let rows = sqlx::query(
        r#"
        SELECT
          a.appointment_id,
          a.start_at,
          a.end_at,
          a.status,
          a.priority,
          a.color_override,
          a.note,

          p.patient_id,
          p.first_name AS p_first,
          p.last_name  AS p_last,
          p.register_number AS p_reg,

          d.employee_id AS d_id,
          d.employee_display_number AS d_no,
          d.first_name AS d_first,
          d.last_name  AS d_last,

          api.service_id AS svc_id,
          api.qty AS svc_qty,
          sc.display_name AS svc_name,
          sc.display_number AS svc_no

        FROM appointment a
        JOIN patient p ON p.patient_id = a.patient_id
        JOIN employee d ON d.employee_id = a.doctor_employee_id
        LEFT JOIN appointment_plan_item api ON api.appointment_id = a.appointment_id
        LEFT JOIN service_catalog sc ON sc.service_id = api.service_id

        WHERE a.doctor_employee_id = $1
          AND a.start_at >= $2
          AND a.start_at <  $3

        ORDER BY a.start_at ASC, sc.display_number ASC
        "#,
    )
    .bind(doctor_employee_id)
    .bind(start_ts)
    .bind(end_ts)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(ApiOk {
        data: fold_rows_into_blocks(rows)?,
    }))
}

/* ============================================================
   GET /appointments/{id}
   ============================================================ */

pub async fn get_appointment(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(appointment_id): Path<Uuid>,
) -> Result<Json<ApiOk<AppointmentBlockDto>>, ApiError> {
    // Authorization: doctors can only view their own appointment
    // admin/manager/receptionist can view any.
    let rows = sqlx::query(
        r#"
        SELECT
          a.appointment_id,
          a.start_at,
          a.end_at,
          a.status,
          a.priority,
          a.color_override,
          a.note,

          p.patient_id,
          p.first_name AS p_first,
          p.last_name  AS p_last,
          p.register_number AS p_reg,

          d.employee_id AS d_id,
          d.employee_display_number AS d_no,
          d.first_name AS d_first,
          d.last_name  AS d_last,

          api.service_id AS svc_id,
          api.qty AS svc_qty,
          sc.display_name AS svc_name,
          sc.display_number AS svc_no

        FROM appointment a
        JOIN patient p ON p.patient_id = a.patient_id
        JOIN employee d ON d.employee_id = a.doctor_employee_id
        LEFT JOIN appointment_plan_item api ON api.appointment_id = a.appointment_id
        LEFT JOIN service_catalog sc ON sc.service_id = api.service_id

        WHERE a.appointment_id = $1

        ORDER BY sc.display_number ASC
        "#,
    )
    .bind(appointment_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    if rows.is_empty() {
        return Err(ApiError::BadRequest("NOT_FOUND", "appointment not found".into()));
    }

    let blocks = fold_rows_into_blocks(rows)?;
    let block = blocks.into_iter().next().unwrap();

    if is_doctor(&auth) {
        let my_emp = resolve_doctor_employee_id_by_user_id(&state, auth.user_id).await?;
        if block.doctor.id != my_emp {
            return Err(ApiError::Forbidden(
                "FORBIDDEN",
                "Doctor can only view their own appointment".into(),
            ));
        }
    }

    Ok(Json(ApiOk { data: block }))
}

/* ============================================================
   POST /appointments (create)
   ============================================================ */

#[derive(Debug, Deserialize)]
pub struct CreateAppointmentRequest {
    pub patient_id: Uuid,
    pub doctor_employee_id: Uuid,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub assistant_employee_id: Option<Uuid>,
    pub receptionist_employee_id: Option<Uuid>,
    pub note: Option<String>,
    pub priority: Option<i16>, // 0 normal, 1 asap
    pub is_new_patient: Option<bool>,
    pub planned_items: Option<Vec<CreatePlanItem>>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePlanItem {
    pub service_id: Uuid,
    pub qty: Option<i32>,
    pub note: Option<String>,
}

pub async fn create_appointment(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(req): Json<CreateAppointmentRequest>,
) -> Result<Json<ApiOk<AppointmentBlockDto>>, ApiError> {
    ensure_manage(&auth)?;

    if req.end_at <= req.start_at {
        return Err(ApiError::BadRequest("VALIDATION_ERROR", "end_at must be > start_at".into()));
    }
    let priority = req.priority.unwrap_or(0);
    if priority != 0 && priority != 1 {
        return Err(ApiError::BadRequest("VALIDATION_ERROR", "priority must be 0 or 1".into()));
    }

    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    let row = sqlx::query(
        r#"
        INSERT INTO appointment (
          patient_id,
          doctor_employee_id,
          receptionist_employee_id,
          assistant_employee_id,
          start_at,
          end_at,
          status,
          is_new_patient,
          priority,
          note,
          created_by_user_id,
          updated_by_user_id
        )
        VALUES ($1,$2,$3,$4,$5,$6, 0, $7, $8, $9, $10, $10)
        RETURNING appointment_id
        "#,
    )
    .bind(req.patient_id)
    .bind(req.doctor_employee_id)
    .bind(req.receptionist_employee_id)
    .bind(req.assistant_employee_id)
    .bind(req.start_at)
    .bind(req.end_at)
    .bind(req.is_new_patient.unwrap_or(false))
    .bind(priority)
    .bind(req.note)
    .bind(auth.user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| ApiError::BadRequest("APPOINTMENT_CREATE_FAILED", format!("{e}")))?;

    let appointment_id: Uuid = row
        .try_get("appointment_id")
        .map_err(|e| ApiError::Internal(format!("row decode error: {e}")))?;

    // planned items
    if let Some(items) = req.planned_items {
        for it in items {
            let qty = it.qty.unwrap_or(1);
            if qty <= 0 {
                return Err(ApiError::BadRequest("VALIDATION_ERROR", "qty must be > 0".into()));
            }
            sqlx::query(
                r#"
                INSERT INTO appointment_plan_item (appointment_id, service_id, qty, note)
                VALUES ($1,$2,$3,$4)
                "#,
            )
            .bind(appointment_id)
            .bind(it.service_id)
            .bind(qty)
            .bind(it.note)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::BadRequest("PLAN_ITEM_CREATE_FAILED", format!("{e}")))?;
        }
    }

    tx.commit()
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    // return detail
    get_appointment(State(state), auth, Path(appointment_id)).await
}

/* ============================================================
   PATCH /appointments/{id}
   ============================================================ */

#[derive(Debug, Deserialize)]
pub struct PatchAppointmentRequest {
    pub start_at: Option<DateTime<Utc>>,
    pub end_at: Option<DateTime<Utc>>,
    pub status: Option<i16>,
    pub priority: Option<i16>,
    pub assistant_employee_id: Option<Option<Uuid>>,
    pub receptionist_employee_id: Option<Option<Uuid>>,
    pub note: Option<Option<String>>,
    pub color_override: Option<Option<i32>>,
}

pub async fn patch_appointment(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(appointment_id): Path<Uuid>,
    Json(req): Json<PatchAppointmentRequest>,
) -> Result<Json<ApiOk<AppointmentBlockDto>>, ApiError> {
    ensure_manage(&auth)?;

    // status enum in your migration: [0..5]
    if let Some(s) = req.status {
        if !(0..=5).contains(&s) {
            return Err(ApiError::BadRequest("VALIDATION_ERROR", "invalid status".into()));
        }
    }
    if let Some(p) = req.priority {
        if p != 0 && p != 1 {
            return Err(ApiError::BadRequest("VALIDATION_ERROR", "priority must be 0 or 1".into()));
        }
    }

    // We update with COALESCE logic; but for "nullable set to null", we used Option<Option<T>>
    let row = sqlx::query(
        r#"
        UPDATE appointment
        SET
          start_at = COALESCE($2, start_at),
          end_at   = COALESCE($3, end_at),
          status   = COALESCE($4, status),
          priority = COALESCE($5, priority),
          assistant_employee_id    = COALESCE($6, assistant_employee_id),
          receptionist_employee_id = COALESCE($7, receptionist_employee_id),
          note           = COALESCE($8, note),
          color_override = COALESCE($9, color_override),
          updated_at = now(),
          updated_by_user_id = $10
        WHERE appointment_id = $1
        RETURNING appointment_id, start_at, end_at
        "#,
    )
    .bind(appointment_id)
    .bind(req.start_at)
    .bind(req.end_at)
    .bind(req.status)
    .bind(req.priority)
    .bind(req.assistant_employee_id.unwrap_or(None))
    .bind(req.receptionist_employee_id.unwrap_or(None))
    .bind(req.note.unwrap_or(None))
    .bind(req.color_override.unwrap_or(None))
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::BadRequest("APPOINTMENT_UPDATE_FAILED", format!("{e}")))?;

    let Some(row) = row else {
        return Err(ApiError::BadRequest("NOT_FOUND", "appointment not found".into()));
    };

    let start_at: DateTime<Utc> = row.try_get("start_at").map_err(|e| ApiError::Internal(format!("{e}")))?;
    let end_at: DateTime<Utc> = row.try_get("end_at").map_err(|e| ApiError::Internal(format!("{e}")))?;
    if end_at <= start_at {
        return Err(ApiError::BadRequest("VALIDATION_ERROR", "end_at must be > start_at".into()));
    }

    get_appointment(State(state), auth, Path(appointment_id)).await
}

/* ============================================================
   Status transitions
   ============================================================ */

pub async fn mark_arrived(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(appointment_id): Path<Uuid>,
) -> Result<Json<ApiOk<AppointmentBlockDto>>, ApiError> {
    ensure_manage(&auth)?;
    sqlx::query(
        r#"
        UPDATE appointment
        SET arrived_at = COALESCE(arrived_at, now()),
            status = 2,
            updated_at = now(),
            updated_by_user_id = $2
        WHERE appointment_id = $1
        "#,
    )
    .bind(appointment_id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::BadRequest("APPOINTMENT_UPDATE_FAILED", format!("{e}")))?;

    get_appointment(State(state), auth, Path(appointment_id)).await
}

pub async fn mark_seated(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(appointment_id): Path<Uuid>,
) -> Result<Json<ApiOk<AppointmentBlockDto>>, ApiError> {
    ensure_manage(&auth)?;
    sqlx::query(
        r#"
        UPDATE appointment
        SET seated_at = COALESCE(seated_at, now()),
            status = 3,
            updated_at = now(),
            updated_by_user_id = $2
        WHERE appointment_id = $1
        "#,
    )
    .bind(appointment_id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::BadRequest("APPOINTMENT_UPDATE_FAILED", format!("{e}")))?;

    get_appointment(State(state), auth, Path(appointment_id)).await
}

pub async fn mark_dismissed(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(appointment_id): Path<Uuid>,
) -> Result<Json<ApiOk<AppointmentBlockDto>>, ApiError> {
    ensure_manage(&auth)?;
    sqlx::query(
        r#"
        UPDATE appointment
        SET dismissed_at = COALESCE(dismissed_at, now()),
            status = 4,
            updated_at = now(),
            updated_by_user_id = $2
        WHERE appointment_id = $1
        "#,
    )
    .bind(appointment_id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::BadRequest("APPOINTMENT_UPDATE_FAILED", format!("{e}")))?;

    get_appointment(State(state), auth, Path(appointment_id)).await
}

/* ============================================================
   PUT /appointments/{id}/plan_items  (replace all)
   ============================================================ */

#[derive(Debug, Deserialize)]
pub struct PutPlanItemsRequest {
    pub items: Vec<CreatePlanItem>,
}

pub async fn put_plan_items(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(appointment_id): Path<Uuid>,
    Json(req): Json<PutPlanItemsRequest>,
) -> Result<Json<ApiOk<AppointmentBlockDto>>, ApiError> {
    ensure_manage(&auth)?;

    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    // delete old
    sqlx::query(r#"DELETE FROM appointment_plan_item WHERE appointment_id = $1"#)
        .bind(appointment_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    // insert new
    for it in req.items {
        let qty = it.qty.unwrap_or(1);
        if qty <= 0 {
            return Err(ApiError::BadRequest("VALIDATION_ERROR", "qty must be > 0".into()));
        }
        sqlx::query(
            r#"
            INSERT INTO appointment_plan_item (appointment_id, service_id, qty, note)
            VALUES ($1,$2,$3,$4)
            "#,
        )
        .bind(appointment_id)
        .bind(it.service_id)
        .bind(qty)
        .bind(it.note)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::BadRequest("PLAN_ITEM_CREATE_FAILED", format!("{e}")))?;
    }

    // touch appointment updated_at
    sqlx::query(
        r#"
        UPDATE appointment
        SET updated_at = now(), updated_by_user_id = $2
        WHERE appointment_id = $1
        "#,
    )
    .bind(appointment_id)
    .bind(auth.user_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    get_appointment(State(state), auth, Path(appointment_id)).await
}

/* ============================================================
   Helper: fold joined rows into appointment blocks
   ============================================================ */

fn fold_rows_into_blocks(rows: Vec<sqlx::postgres::PgRow>) -> Result<Vec<AppointmentBlockDto>, ApiError> {
    use std::collections::BTreeMap;

    // appointment_id -> dto
    let mut map: BTreeMap<Uuid, AppointmentBlockDto> = BTreeMap::new();

    for r in rows {
        let appointment_id: Uuid = r.try_get("appointment_id").map_err(internal_row)?;
        let start_at: DateTime<Utc> = r.try_get("start_at").map_err(internal_row)?;
        let end_at: DateTime<Utc> = r.try_get("end_at").map_err(internal_row)?;
        let status: i16 = r.try_get("status").map_err(internal_row)?;
        let priority: i16 = r.try_get("priority").map_err(internal_row)?;
        let color_override: Option<i32> = r.try_get("color_override").map_err(internal_row)?;
        let note: Option<String> = r.try_get("note").map_err(internal_row)?;

        let p_id: Uuid = r.try_get("patient_id").map_err(internal_row)?;
        let p_first: String = r.try_get("p_first").map_err(internal_row)?;
        let p_last: String = r.try_get("p_last").map_err(internal_row)?;
        let p_reg: Option<i64> = r.try_get("p_reg").ok(); // register_number might be numeric or null

        let d_id: Uuid = r.try_get("d_id").map_err(internal_row)?;
        let d_no: i64 = r.try_get("d_no").map_err(internal_row)?;
        let d_first: String = r.try_get("d_first").map_err(internal_row)?;
        let d_last: String = r.try_get("d_last").map_err(internal_row)?;

        let entry = map.entry(appointment_id).or_insert_with(|| AppointmentBlockDto {
            appointment_id,
            start_at,
            end_at,
            status,
            priority,
            color_override,
            note: note.clone(),
            patient: PersonBrief {
                id: p_id,
                display: format!("{p_first} {p_last}"),
                number: p_reg,
            },
            doctor: PersonBrief {
                id: d_id,
                display: format!("{d_first} {d_last}"),
                number: Some(d_no),
            },
            planned_items: vec![],
            planned_summary: String::new(),
        });

        // collect planned item if exists
        let svc_id: Option<Uuid> = r.try_get("svc_id").ok();
        if let Some(service_id) = svc_id {
            let qty: i32 = r.try_get("svc_qty").unwrap_or(1);
            let name: String = r.try_get("svc_name").unwrap_or_else(|_| "Service".into());
            entry.planned_items.push(AppointmentPlanItemDto {
                service_id,
                display_name: name,
                qty,
            });
        }
    }

    // build planned_summary (e.g. "Cleaning + X-Ray")
    for v in map.values_mut() {
        if v.planned_items.is_empty() {
            v.planned_summary = "(no planned items)".into();
        } else {
            // expand qty into repeated labels if qty>1 (optional)
            let mut parts: Vec<String> = vec![];
            for it in &v.planned_items {
                if it.qty <= 1 {
                    parts.push(it.display_name.clone());
                } else {
                    parts.push(format!("{}×{}", it.display_name, it.qty));
                }
            }
            v.planned_summary = parts.join(" + ");
        }
    }

    Ok(map.into_values().collect())
}

fn internal_row(e: sqlx::Error) -> ApiError {
    ApiError::Internal(format!("row decode error: {e}"))
}
