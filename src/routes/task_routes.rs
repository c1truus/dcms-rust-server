// src/routes/task_routes.rs

use axum::{
    extract::{Path, Query, State},
    routing::{get, patch, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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

fn is_admin(auth: &AuthContext) -> bool { auth.role == 1 }
fn is_manager(auth: &AuthContext) -> bool { auth.role == 2 }
fn is_doctor(auth: &AuthContext) -> bool { auth.role == 3 }
fn is_receptionist(auth: &AuthContext) -> bool { auth.role == 4 }

fn can_manage_tasks(auth: &AuthContext) -> bool {
    is_admin(auth) || is_manager(auth) || is_receptionist(auth)
}

fn can_create_tasks(auth: &AuthContext) -> bool {
    can_manage_tasks(auth) || is_doctor(auth)
}

fn ensure_manage(auth: &AuthContext) -> Result<(), ApiError> {
    if can_manage_tasks(auth) {
        Ok(())
    } else {
        Err(ApiError::Forbidden(
            "FORBIDDEN",
            "Only admin/manager/receptionist can manage tasks".into(),
        ))
    }
}

fn ensure_create(auth: &AuthContext) -> Result<(), ApiError> {
    if can_create_tasks(auth) {
        Ok(())
    } else {
        Err(ApiError::Forbidden(
            "FORBIDDEN",
            "Only staff can create tasks".into(),
        ))
    }
}

async fn resolve_employee_id_by_user_id(state: &AppState, user_id: Uuid) -> Result<Uuid, ApiError> {
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
            "This user has no employee profile".into(),
        ));
    };

    let employee_id: Uuid = row
        .try_get("employee_id")
        .map_err(|e| ApiError::Internal(format!("row decode error: {e}")))?;

    Ok(employee_id)
}

/* ============================================================
   Router
   ============================================================ */

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tasks", post(create_task))
        .route("/tasks/inbox", get(list_tasks_inbox))
        .route("/tasks/my", get(list_tasks_my))
        .route("/tasks/created", get(list_tasks_created))
        .route("/tasks/{task_id}", get(get_task))
        .route("/tasks/{task_id}", patch(patch_task))
        .route("/tasks/{task_id}/assign", post(assign_task))
        .route("/tasks/{task_id}/start", post(start_task))
        .route("/tasks/{task_id}/complete", post(complete_task))
        .route("/tasks/{task_id}/cancel", post(cancel_task))
}

/* ============================================================
   DTOs
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
pub struct TaskDto {
    pub task_id: Uuid,
    pub task_type: String,
    pub status: i16,
    pub priority: i16,
    pub due_at: Option<DateTime<Utc>>,
    pub title: String,
    pub details: Option<String>,

    pub patient: Option<PersonBrief>,
    pub appointment_id: Option<Uuid>,

    pub created_by: PersonBrief,
    pub assigned_to: Option<PersonBrief>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub canceled_at: Option<DateTime<Utc>>,
}

/* ============================================================
   Helpers: authorization + fetch
   ============================================================ */

async fn fetch_task_with_joins(state: &AppState, task_id: Uuid) -> Result<TaskDto, ApiError> {
    let row = sqlx::query(
        r#"
        SELECT
          t.task_id,
          t.task_type,
          t.status,
          t.priority,
          t.due_at,
          t.title,
          t.details,
          t.patient_id,
          t.appointment_id,
          t.created_at,
          t.updated_at,
          t.started_at,
          t.completed_at,
          t.canceled_at,

          cb.employee_id AS cb_id,
          cb.employee_display_number AS cb_no,
          cb.first_name AS cb_first,
          cb.last_name  AS cb_last,

          at.employee_id AS at_id,
          at.employee_display_number AS at_no,
          at.first_name AS at_first,
          at.last_name  AS at_last,

          p.patient_id AS p_id,
          p.first_name AS p_first,
          p.last_name  AS p_last,
          p.register_number AS p_reg

        FROM task t
        JOIN employee cb ON cb.employee_id = t.created_by_employee_id
        LEFT JOIN employee at ON at.employee_id = t.assigned_to_employee_id
        LEFT JOIN patient p ON p.patient_id = t.patient_id
        WHERE t.task_id = $1
        "#,
    )
    .bind(task_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    let Some(r) = row else {
        return Err(ApiError::BadRequest("NOT_FOUND", "task not found".into()));
    };

    let task_id: Uuid = r.try_get("task_id").map_err(internal_row)?;
    let task_type: String = r.try_get("task_type").map_err(internal_row)?;
    let status: i16 = r.try_get("status").map_err(internal_row)?;
    let priority: i16 = r.try_get("priority").map_err(internal_row)?;
    let due_at: Option<DateTime<Utc>> = r.try_get("due_at").ok();
    let title: String = r.try_get("title").map_err(internal_row)?;
    let details: Option<String> = r.try_get("details").ok();
    let appointment_id: Option<Uuid> = r.try_get("appointment_id").ok();

    let created_at: DateTime<Utc> = r.try_get("created_at").map_err(internal_row)?;
    let updated_at: DateTime<Utc> = r.try_get("updated_at").map_err(internal_row)?;
    let started_at: Option<DateTime<Utc>> = r.try_get("started_at").ok();
    let completed_at: Option<DateTime<Utc>> = r.try_get("completed_at").ok();
    let canceled_at: Option<DateTime<Utc>> = r.try_get("canceled_at").ok();

    let cb_id: Uuid = r.try_get("cb_id").map_err(internal_row)?;
    let cb_no: i64 = r.try_get("cb_no").map_err(internal_row)?;
    let cb_first: String = r.try_get("cb_first").map_err(internal_row)?;
    let cb_last: String = r.try_get("cb_last").map_err(internal_row)?;

    let created_by = PersonBrief {
        id: cb_id,
        display: format!("{cb_first} {cb_last}"),
        number: Some(cb_no),
    };

    let at_id: Option<Uuid> = r.try_get("at_id").ok();
    let assigned_to = if let Some(aid) = at_id {
        let at_no: i64 = r.try_get("at_no").map_err(internal_row)?;
        let at_first: String = r.try_get("at_first").map_err(internal_row)?;
        let at_last: String = r.try_get("at_last").map_err(internal_row)?;
        Some(PersonBrief {
            id: aid,
            display: format!("{at_first} {at_last}"),
            number: Some(at_no),
        })
    } else {
        None
    };

    let p_id: Option<Uuid> = r.try_get("p_id").ok();
    let patient = if let Some(pid) = p_id {
        let p_first: String = r.try_get("p_first").map_err(internal_row)?;
        let p_last: String = r.try_get("p_last").map_err(internal_row)?;
        let p_reg: Option<String> = r.try_get("p_reg").ok();
        // register_number is TEXT in your schema; we keep number as Option<i64>, so we won't parse it here.
        // UI can use display; register_number can be added as separate string later if you want.
        let _ = p_reg;
        Some(PersonBrief {
            id: pid,
            display: format!("{p_first} {p_last}"),
            number: None,
        })
    } else {
        None
    };

    Ok(TaskDto {
        task_id,
        task_type,
        status,
        priority,
        due_at,
        title,
        details,
        patient,
        appointment_id,
        created_by,
        assigned_to,
        created_at,
        updated_at,
        started_at,
        completed_at,
        canceled_at,
    })
}

async fn ensure_can_view_task(
    state: &AppState,
    auth: &AuthContext,
    task_id: Uuid,
) -> Result<TaskDto, ApiError> {
    let dto = fetch_task_with_joins(state, task_id).await?;

    if can_manage_tasks(auth) {
        return Ok(dto);
    }

    // doctor: can view if created_by == me OR assigned_to == me
    if is_doctor(auth) {
        let my_emp = resolve_employee_id_by_user_id(state, auth.user_id).await?;
        let created_ok = dto.created_by.id == my_emp;
        let assigned_ok = dto.assigned_to.as_ref().map(|x| x.id) == Some(my_emp);
        if created_ok || assigned_ok {
            return Ok(dto);
        }
        return Err(ApiError::Forbidden("FORBIDDEN", "cannot view this task".into()));
    }

    Err(ApiError::Forbidden("FORBIDDEN", "cannot view this task".into()))
}

/* ============================================================
   POST /tasks
   ============================================================ */

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub task_type: String,
    pub title: String,
    pub details: Option<String>,

    pub priority: Option<i16>, // 0 normal, 1 high, 2 urgent
    pub due_at: Option<DateTime<Utc>>,

    pub assigned_to_employee_id: Option<Uuid>,
    pub patient_id: Option<Uuid>,
    pub appointment_id: Option<Uuid>,
}

pub async fn create_task(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<ApiOk<TaskDto>>, ApiError> {
    ensure_create(&auth)?;

    if req.title.trim().is_empty() {
        return Err(ApiError::BadRequest("VALIDATION_ERROR", "title is required".into()));
    }
    if req.task_type.trim().is_empty() {
        return Err(ApiError::BadRequest("VALIDATION_ERROR", "task_type is required".into()));
    }

    let priority = req.priority.unwrap_or(0);
    if !(0..=2).contains(&priority) {
        return Err(ApiError::BadRequest("VALIDATION_ERROR", "priority must be 0..2".into()));
    }

    let created_by_employee_id = resolve_employee_id_by_user_id(&state, auth.user_id).await?;

    let row = sqlx::query(
        r#"
        INSERT INTO task (
          created_by_employee_id,
          assigned_to_employee_id,
          patient_id,
          appointment_id,
          task_type,
          status,
          priority,
          due_at,
          title,
          details,
          updated_by_employee_id
        )
        VALUES ($1,$2,$3,$4,$5,0,$6,$7,$8,$9,$1)
        RETURNING task_id
        "#,
    )
    .bind(created_by_employee_id)
    .bind(req.assigned_to_employee_id)
    .bind(req.patient_id)
    .bind(req.appointment_id)
    .bind(req.task_type.trim())
    .bind(priority)
    .bind(req.due_at)
    .bind(req.title.trim())
    .bind(req.details)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::BadRequest("TASK_CREATE_FAILED", format!("{e}")))?;

    let task_id: Uuid = row.try_get("task_id").map_err(internal_row)?;
    let dto = ensure_can_view_task(&state, &auth, task_id).await?;
    Ok(Json(ApiOk { data: dto }))
}

/* ============================================================
   GET /tasks/{id}
   ============================================================ */

pub async fn get_task(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(task_id): Path<Uuid>,
) -> Result<Json<ApiOk<TaskDto>>, ApiError> {
    let dto = ensure_can_view_task(&state, &auth, task_id).await?;
    Ok(Json(ApiOk { data: dto }))
}

/* ============================================================
   Lists
   ============================================================ */

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub status: Option<i16>,  // optional filter
    pub limit: Option<i64>,   // default 50
    pub offset: Option<i64>,  // default 0
}

async fn list_tasks_common(
    state: &AppState,
    where_sql: &str,
    binds: Vec<Uuid>,
    q: &ListQuery,
) -> Result<Vec<TaskDto>, ApiError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).max(0);

    // NOTE: This is a simple approach without dynamic SQL builder crate.
    // We only support optional status filter in Phase 1.
    let mut sql = format!(
        r#"
        SELECT
          t.task_id
        FROM task t
        {where_sql}
        "#
    );

    if q.status.is_some() {
        sql.push_str(" AND t.status = $XSTATUS ");
    }

    sql.push_str(" ORDER BY COALESCE(t.due_at, t.created_at) ASC, t.created_at ASC ");
    sql.push_str(" LIMIT $XLIMIT OFFSET $XOFFSET ");

    // Replace placeholders with positional args
    // binds are $1..$n, then optional status, then limit, offset.
    let mut idx = 1;
    for _ in &binds {
        idx += 1;
    }
    let status_idx = idx;
    let limit_idx = if q.status.is_some() { status_idx + 1 } else { status_idx };
    let offset_idx = limit_idx + 1;

    let sql = sql
        .replace("$XSTATUS", &status_idx.to_string())
        .replace("$XLIMIT", &limit_idx.to_string())
        .replace("$XOFFSET", &offset_idx.to_string());

    let mut query = sqlx::query(&sql);
    for b in binds {
        query = query.bind(b);
    }
    if let Some(st) = q.status {
        query = query.bind(st);
    }
    query = query.bind(limit).bind(offset);

    let rows = query
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let task_id: Uuid = r.try_get("task_id").map_err(internal_row)?;
        out.push(fetch_task_with_joins(state, task_id).await?);
    }
    Ok(out)
}

// GET /tasks/inbox : unassigned open/in_progress only
pub async fn list_tasks_inbox(
    State(state): State<AppState>,
    auth: AuthContext,
    Query(q): Query<ListQuery>,
) -> Result<Json<ApiOk<Vec<TaskDto>>>, ApiError> {
    ensure_manage(&auth)?;

    let items = list_tasks_common(
        &state,
        "WHERE t.assigned_to_employee_id IS NULL AND t.status IN (0,1)",
        vec![],
        &q,
    )
    .await?;

    Ok(Json(ApiOk { data: items }))
}

// GET /tasks/my : assigned to me, open/in_progress only
pub async fn list_tasks_my(
    State(state): State<AppState>,
    auth: AuthContext,
    Query(q): Query<ListQuery>,
) -> Result<Json<ApiOk<Vec<TaskDto>>>, ApiError> {
    let my_emp = resolve_employee_id_by_user_id(&state, auth.user_id).await?;

    let items = list_tasks_common(
        &state,
        "WHERE t.assigned_to_employee_id = $1 AND t.status IN (0,1)",
        vec![my_emp],
        &q,
    )
    .await?;

    Ok(Json(ApiOk { data: items }))
}

// GET /tasks/created : created by me (any status)
pub async fn list_tasks_created(
    State(state): State<AppState>,
    auth: AuthContext,
    Query(q): Query<ListQuery>,
) -> Result<Json<ApiOk<Vec<TaskDto>>>, ApiError> {
    let my_emp = resolve_employee_id_by_user_id(&state, auth.user_id).await?;

    let items = list_tasks_common(
        &state,
        "WHERE t.created_by_employee_id = $1",
        vec![my_emp],
        &q,
    )
    .await?;

    Ok(Json(ApiOk { data: items }))
}

/* ============================================================
   PATCH /tasks/{id}
   ============================================================ */

#[derive(Debug, Deserialize)]
pub struct PatchTaskRequest {
    pub task_type: Option<String>,
    pub title: Option<String>,
    pub details: Option<Option<String>>,
    pub priority: Option<i16>,
    pub due_at: Option<Option<DateTime<Utc>>>,

    pub assigned_to_employee_id: Option<Option<Uuid>>,
    pub patient_id: Option<Option<Uuid>>,
    pub appointment_id: Option<Option<Uuid>>,

    pub status: Option<i16>, // allow manage-role only
}

pub async fn patch_task(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(task_id): Path<Uuid>,
    Json(req): Json<PatchTaskRequest>,
) -> Result<Json<ApiOk<TaskDto>>, ApiError> {
    // Ensure view first (also ensures existence)
    let current = ensure_can_view_task(&state, &auth, task_id).await?;

    let my_emp = resolve_employee_id_by_user_id(&state, auth.user_id).await?;

    // manage roles: can patch anything
    // doctor: can only patch if created_by == me, and cannot reassign others / set arbitrary status
    let doctor_limited = is_doctor(&auth) && !can_manage_tasks(&auth);

    if doctor_limited && current.created_by.id != my_emp {
        return Err(ApiError::Forbidden(
            "FORBIDDEN",
            "Doctor can only edit tasks they created".into(),
        ));
    }

    if doctor_limited {
        // doctor cannot reassign, cannot change status directly via patch
        if req.assigned_to_employee_id.is_some() || req.status.is_some() {
            return Err(ApiError::Forbidden(
                "FORBIDDEN",
                "Doctor cannot assign tasks or set status via patch".into(),
            ));
        }
    }

    if let Some(p) = req.priority {
        if !(0..=2).contains(&p) {
            return Err(ApiError::BadRequest("VALIDATION_ERROR", "priority must be 0..2".into()));
        }
    }
    if let Some(st) = req.status {
        if !(0..=3).contains(&st) {
            return Err(ApiError::BadRequest("VALIDATION_ERROR", "status must be 0..3".into()));
        }
    }

    let row = sqlx::query(
        r#"
        UPDATE task
        SET
          task_type = COALESCE($2, task_type),
          title     = COALESCE($3, title),
          details   = COALESCE($4, details),
          priority  = COALESCE($5, priority),
          due_at    = COALESCE($6, due_at),

          assigned_to_employee_id = COALESCE($7, assigned_to_employee_id),
          patient_id              = COALESCE($8, patient_id),
          appointment_id          = COALESCE($9, appointment_id),

          status = COALESCE($10, status),
          updated_by_employee_id = $11
        WHERE task_id = $1
        RETURNING task_id
        "#,
    )
    .bind(task_id)
    .bind(req.task_type.as_deref())
    .bind(req.title.as_deref())
    .bind(req.details.unwrap_or(None))
    .bind(req.priority)
    .bind(req.due_at.unwrap_or(None))
    .bind(req.assigned_to_employee_id.unwrap_or(None))
    .bind(req.patient_id.unwrap_or(None))
    .bind(req.appointment_id.unwrap_or(None))
    .bind(req.status)
    .bind(my_emp)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::BadRequest("TASK_UPDATE_FAILED", format!("{e}")))?;

    let Some(_row) = row else {
        return Err(ApiError::BadRequest("NOT_FOUND", "task not found".into()));
    };

    let dto = ensure_can_view_task(&state, &auth, task_id).await?;
    Ok(Json(ApiOk { data: dto }))
}

/* ============================================================
   POST /tasks/{id}/assign
   ============================================================ */

#[derive(Debug, Deserialize)]
pub struct AssignTaskRequest {
    pub assigned_to_employee_id: Option<Uuid>, // null = unassign
}

pub async fn assign_task(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(task_id): Path<Uuid>,
    Json(req): Json<AssignTaskRequest>,
) -> Result<Json<ApiOk<TaskDto>>, ApiError> {
    ensure_manage(&auth)?;
    let my_emp = resolve_employee_id_by_user_id(&state, auth.user_id).await?;

    sqlx::query(
        r#"
        UPDATE task
        SET assigned_to_employee_id = $2,
            updated_by_employee_id = $3
        WHERE task_id = $1
        "#,
    )
    .bind(task_id)
    .bind(req.assigned_to_employee_id)
    .bind(my_emp)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::BadRequest("TASK_ASSIGN_FAILED", format!("{e}")))?;

    let dto = ensure_can_view_task(&state, &auth, task_id).await?;
    Ok(Json(ApiOk { data: dto }))
}

/* ============================================================
   Status transitions
   ============================================================ */

pub async fn start_task(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(task_id): Path<Uuid>,
) -> Result<Json<ApiOk<TaskDto>>, ApiError> {
    // start: manage role OR assigned person OR creator (doctor)
    let dto = ensure_can_view_task(&state, &auth, task_id).await?;
    let my_emp = resolve_employee_id_by_user_id(&state, auth.user_id).await?;

    if !can_manage_tasks(&auth) {
        let assigned_ok = dto.assigned_to.as_ref().map(|x| x.id) == Some(my_emp);
        let created_ok = dto.created_by.id == my_emp;
        if !(assigned_ok || created_ok) {
            return Err(ApiError::Forbidden("FORBIDDEN", "cannot start this task".into()));
        }
    }

    sqlx::query(
        r#"
        UPDATE task
        SET status = 1,
            started_at = COALESCE(started_at, now()),
            updated_by_employee_id = $2
        WHERE task_id = $1
          AND status IN (0,1)
        "#,
    )
    .bind(task_id)
    .bind(my_emp)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::BadRequest("TASK_START_FAILED", format!("{e}")))?;

    let dto = ensure_can_view_task(&state, &auth, task_id).await?;
    Ok(Json(ApiOk { data: dto }))
}

pub async fn complete_task(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(task_id): Path<Uuid>,
) -> Result<Json<ApiOk<TaskDto>>, ApiError> {
    // complete: manage role OR assigned person
    let dto = ensure_can_view_task(&state, &auth, task_id).await?;
    let my_emp = resolve_employee_id_by_user_id(&state, auth.user_id).await?;

    if !can_manage_tasks(&auth) {
        let assigned_ok = dto.assigned_to.as_ref().map(|x| x.id) == Some(my_emp);
        if !assigned_ok {
            return Err(ApiError::Forbidden("FORBIDDEN", "only assignee can complete task".into()));
        }
    }

    sqlx::query(
        r#"
        UPDATE task
        SET status = 2,
            completed_at = COALESCE(completed_at, now()),
            updated_by_employee_id = $2
        WHERE task_id = $1
          AND status IN (0,1)
        "#,
    )
    .bind(task_id)
    .bind(my_emp)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::BadRequest("TASK_COMPLETE_FAILED", format!("{e}")))?;

    let dto = ensure_can_view_task(&state, &auth, task_id).await?;
    Ok(Json(ApiOk { data: dto }))
}

pub async fn cancel_task(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(task_id): Path<Uuid>,
) -> Result<Json<ApiOk<TaskDto>>, ApiError> {
    // cancel: manage role OR creator (doctor)
    let dto = ensure_can_view_task(&state, &auth, task_id).await?;
    let my_emp = resolve_employee_id_by_user_id(&state, auth.user_id).await?;

    if !can_manage_tasks(&auth) {
        if !(is_doctor(&auth) && dto.created_by.id == my_emp) {
            return Err(ApiError::Forbidden(
                "FORBIDDEN",
                "Doctor can only cancel tasks they created".into(),
            ));
        }
    }

    sqlx::query(
        r#"
        UPDATE task
        SET status = 3,
            canceled_at = COALESCE(canceled_at, now()),
            updated_by_employee_id = $2
        WHERE task_id = $1
          AND status IN (0,1)
        "#,
    )
    .bind(task_id)
    .bind(my_emp)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::BadRequest("TASK_CANCEL_FAILED", format!("{e}")))?;

    let dto = ensure_can_view_task(&state, &auth, task_id).await?;
    Ok(Json(ApiOk { data: dto }))
}

/* ============================================================
   misc
   ============================================================ */

fn internal_row(e: sqlx::Error) -> ApiError {
    ApiError::Internal(format!("row decode error: {e}"))
}
