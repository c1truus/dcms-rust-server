use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use chrono::{Duration, Utc};

use axum::extract::Path;
use uuid::Uuid;
use serde::{Deserialize, Serialize};

use crate::{
    auth::{generate_access_token, hash_access_token, verify_password, hash_password},
    error::ApiError,
    middleware::auth_context::AuthContext,
    models::{role_to_string, *},
};

// Session type according to migrations/003_session_token.sql
const SESSION_TYPE_UNDEFINED: i16 = 0;
const SESSION_TYPE_USER_PORTAL: i16 = 1;
const SESSION_TYPE_PATIENT_WEB: i16 = 2;
const SESSION_TYPE_DCMSHQ: i16 = 3;

// Safety limits (can be moved to config later)
const MAX_EXTEND_HOURS: i64 = 24 * 30; // 30 days
const DEFAULT_PATIENT_TTL_HOURS: i64 = 24 * 3; // 3 days

fn is_known_session_type(st: i16) -> bool {
    matches!(
        st,
        SESSION_TYPE_UNDEFINED
            | SESSION_TYPE_USER_PORTAL
            | SESSION_TYPE_PATIENT_WEB
            | SESSION_TYPE_DCMSHQ
    )
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        // Future: patient portal login (session_type=2)
        .route("/patient/login", post(patient_login))
        .route("/me", get(me))
        .route("/logout", post(logout))
        // Convenience: revoke all other sessions but keep the current one
        .route("/logout_all_except_current", post(logout_all_except_current))
        // Rotate access token for the current session (invalidates old token immediately)
        .route("/refresh", post(refresh))
        // sessions (you already added these)
        .route("/sessions", get(list_sessions))
        .route("/sessions/{session_token_id}", get(get_session))
        .route("/sessions/{session_token_id}/extend", post(extend_session))
        .route("/sessions/revoke_all", post(revoke_all_sessions))
        .route("/sessions/{session_token_id}/revoke", post(revoke_session))
        // Admin-only: create an impersonation session for a target user
        .route("/impersonate/{user_id}", post(impersonate))
        // NEW: password management
        .route("/change_password", post(change_password))
        .route("/reset_password", post(reset_password))
}


async fn load_clinic_name(state: &AppState) -> Result<String, ApiError> {
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

    Ok(clinic_name.unwrap_or_else(|| "Clinic".to_string()))
}

async fn login_with_type(
    state: &AppState,
    req: &LoginRequest,
    session_type: i16,
    required_role: Option<i16>,
) -> Result<LoginResponse, ApiError> {
    let username = req.username.trim();
    if username.is_empty() || req.password.is_empty() {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "username and password are required".into(),
        ));
    }
    if !is_known_session_type(session_type) {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            format!("unknown session_type: {session_type}"),
        ));
    }


    // 1) Load dcms_user
    let dcms_user: UserRow = sqlx::query_as::<_, UserRow>(
        r#"
        SELECT user_id, username, display_name, password_hash, roles, is_active
        FROM "dcms_user"
        WHERE username = $1
        "#,
    )
    .bind(username)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    .ok_or_else(ApiError::invalid_credentials)?;

    if !dcms_user.is_active {
        return Err(ApiError::Forbidden(
            "FORBIDDEN",
            "Account is disabled".into(),
        ));
    }

    if let Some(rr) = required_role {
        if dcms_user.roles != rr {
            return Err(ApiError::Forbidden(
                "FORBIDDEN",
                "Account type not allowed for this login".into(),
            ));
        }
    }

    // 2) Verify password
    if !verify_password(&req.password, &dcms_user.password_hash) {
        return Err(ApiError::invalid_credentials());
    }

    // 3) Load clinic name (singleton)
    let clinic_name = load_clinic_name(state).await?;

    // 4) Create session_token
    let access_token = generate_access_token();
    let token_hash = hash_access_token(&access_token);

    let ttl_hours = if session_type == SESSION_TYPE_PATIENT_WEB {
        DEFAULT_PATIENT_TTL_HOURS
    } else if req.remember_me.unwrap_or(false) {
        // Example: 7 days
        24 * 7
    } else {
        state.session_ttl_hours
    };

    let expires_at = Utc::now() + Duration::hours(ttl_hours);

    let session: SessionTokenRow = sqlx::query_as::<_, SessionTokenRow>(
        r#"
        INSERT INTO session_token
            (user_id, session_token_hash, session_type, device_name, expires_at)
        VALUES
            ($1, $2, $3, $4, $5)
        RETURNING session_token_id, user_id, expires_at
        "#,
    )
    .bind(dcms_user.user_id)
    .bind(&token_hash)
    .bind(session_type)
    .bind(req.device_name.as_deref())
    .bind(expires_at)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(LoginResponse {
        data: LoginResponseData {
            access_token,
            expires_at: session.expires_at,
            dcms_user: UserProfile {
                user_id: dcms_user.user_id,
                username: dcms_user.username,
                display_name: dcms_user.display_name,
                roles: vec![role_to_string(dcms_user.roles)],
            },
            clinic: ClinicProfile { clinic_name },
        },
    })
}

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    let resp = login_with_type(&state, &req, SESSION_TYPE_USER_PORTAL, None).await?;
    Ok(Json(resp))
}

/// Patient portal login: same credential shape for now (username/password), but enforces role=patient
/// and uses session_type=2.
pub async fn patient_login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    let resp = login_with_type(&state, &req, SESSION_TYPE_PATIENT_WEB, Some(0)).await?;
    Ok(Json(resp))
}


pub async fn me(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<MeResponse>, ApiError> {
    // Load dcms_user
    let dcms_user: UserRow = sqlx::query_as::<_, UserRow>(
        r#"
        SELECT user_id, username, display_name, password_hash, roles, is_active
        FROM "dcms_user"
        WHERE user_id = $1
        "#,
    )
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    .ok_or_else(ApiError::session_expired)?;

    if !dcms_user.is_active {
        return Err(ApiError::session_expired());
    }

    // Load clinic name (singleton)
    let clinic_name = load_clinic_name(&state).await?;

    // Load session token (ensure still active)
    let session: SessionTokenRow = sqlx::query_as::<_, SessionTokenRow>(
        r#"
        SELECT session_token_id, user_id, expires_at
        FROM session_token
        WHERE session_token_id = $1
          AND user_id = $2
          AND revoked_at IS NULL
          AND expires_at > now()
        "#,
    )
    .bind(auth.session_token_id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    .ok_or_else(ApiError::session_expired)?;

    Ok(Json(MeResponse {
        data: MeResponseData {
            dcms_user: UserProfile {
                user_id: dcms_user.user_id,
                username: dcms_user.username,
                display_name: dcms_user.display_name,
                roles: vec![role_to_string(dcms_user.roles)],
            },
            clinic: ClinicProfile { clinic_name },
            session: SessionInfo {
                session_token_id: session.session_token_id,
                expires_at: session.expires_at,
            },
            message: "login success".into(),
        },
    }))
}

pub async fn logout(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<OkResponse>, ApiError> {
    let rows = sqlx::query(
        r#"
        UPDATE session_token
        SET revoked_at = now()
        WHERE session_token_id = $1
          AND user_id = $2
          AND revoked_at IS NULL
        "#,
    )
    .bind(auth.session_token_id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    if rows.rows_affected() == 0 {
        return Err(ApiError::session_expired());
    }

    Ok(Json(OkResponse {
        data: OkData { ok: true },
    }))
}

/// POST /api/v1/auth/logout_all_except_current
/// Revokes all active sessions for the current user except the one used for this request.
pub async fn logout_all_except_current(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<RevokeAllResponse>, ApiError> {
    // This is basically "revoke_all" but exposed as an explicit UX action.
    let res = sqlx::query(
        r#"
        UPDATE session_token
        SET revoked_at = now()
        WHERE user_id = $1
          AND revoked_at IS NULL
          AND expires_at > now()
          AND session_token_id <> $2
        "#,
    )
    .bind(auth.user_id)
    .bind(auth.session_token_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(RevokeAllResponse {
        data: RevokeAllData {
            ok: true,
            revoked_count: res.rows_affected() as i64,
        },
    }))
}

#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    pub data: RefreshData,
}

#[derive(Debug, Serialize)]
pub struct RefreshData {
    pub ok: bool,
    pub access_token: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub session_token_id: Uuid,
}

/// POST /api/v1/auth/refresh
/// Rotates the access token for the *current* session.
/// This immediately invalidates the old token, but keeps the same session_token_id.
pub async fn refresh(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<RefreshResponse>, ApiError> {
    let new_token = generate_access_token();
    let new_hash = hash_access_token(&new_token);

    let row: Option<(chrono::DateTime<chrono::Utc>,)> = sqlx::query_as(
        r#"
        UPDATE session_token
        SET session_token_hash = $1,
            last_seen_at = now()
        WHERE session_token_id = $2
          AND user_id = $3
          AND revoked_at IS NULL
          AND expires_at > now()
        RETURNING expires_at
        "#,
    )
    .bind(new_hash)
    .bind(auth.session_token_id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    let expires_at = row.ok_or_else(ApiError::session_expired)?.0;

    Ok(Json(RefreshResponse {
        data: RefreshData {
            ok: true,
            access_token: new_token,
            expires_at,
            session_token_id: auth.session_token_id,
        },
    }))
}

// =========================
// Session management
// =========================

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SessionListItem {
    pub session_token_id: Uuid,
    pub session_type: i16,
    pub device_name: Option<String>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct ListSessionsResponse {
    pub data: ListSessionsData,
}

#[derive(Debug, Serialize)]
pub struct ListSessionsData {
    pub sessions: Vec<SessionListItem>,
    pub current_session_token_id: Uuid,
}

pub async fn list_sessions(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<ListSessionsResponse>, ApiError> {
    // "active sessions" only: not revoked, not expired
    let rows: Vec<SessionListItem> = sqlx::query_as::<_, SessionListItem>(
        r#"
        SELECT
            session_token_id,
            session_type,
            device_name,
            expires_at,
            last_seen_at,
            created_at
        FROM session_token
        WHERE user_id = $1
          AND revoked_at IS NULL
          AND expires_at > now()
        ORDER BY last_seen_at DESC NULLS LAST, created_at DESC
        "#,
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(ListSessionsResponse {
        data: ListSessionsData {
            sessions: rows,
            current_session_token_id: auth.session_token_id,
        },
    }))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SessionDetail {
    pub session_token_id: Uuid,
    pub user_id: Uuid,
    pub session_type: i16,
    pub device_name: Option<String>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize)]
pub struct GetSessionResponse {
    pub data: GetSessionData,
}

#[derive(Debug, Serialize)]
pub struct GetSessionData {
    pub session: SessionDetail,
}

/// GET /api/v1/auth/sessions/{session_token_id}
/// Returns one session (must belong to current user; admin/manager can inspect any).
pub async fn get_session(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(session_token_id): Path<Uuid>,
) -> Result<Json<GetSessionResponse>, ApiError> {
    // owner can view own; admin/manager can view any
    let (sql, bind_user): (&str, bool) = if auth.role == 1 || auth.role == 2 {
        (
            r#"
            SELECT session_token_id, user_id, session_type, device_name, expires_at, created_at, last_seen_at, revoked_at
            FROM session_token
            WHERE session_token_id = $1
            "#,
            false,
        )
    } else {
        (
            r#"
            SELECT session_token_id, user_id, session_type, device_name, expires_at, created_at, last_seen_at, revoked_at
            FROM session_token
            WHERE session_token_id = $1
              AND user_id = $2
            "#,
            true,
        )
    };

    let mut q = sqlx::query_as::<_, SessionDetail>(sql).bind(session_token_id);
    if bind_user {
        q = q.bind(auth.user_id);
    }

    let session = q
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
        .ok_or_else(|| ApiError::BadRequest("NOT_FOUND", "session not found".into()))?;

    Ok(Json(GetSessionResponse {
        data: GetSessionData { session },
    }))
}

#[derive(Debug, Deserialize)]
pub struct ExtendSessionRequest {
    /// Number of hours to extend, counted from max(now, current expires_at).
    /// If omitted, defaults to `state.session_ttl_hours` (staff) or patient default.
    pub extend_hours: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ExtendSessionResponse {
    pub data: ExtendSessionData,
}

#[derive(Debug, Serialize)]
pub struct ExtendSessionData {
    pub ok: bool,
    pub session_token_id: Uuid,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// POST /api/v1/auth/sessions/{session_token_id}/extend
/// Extends the expiry for a session (must be your own; admin/manager can extend any).
pub async fn extend_session(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(session_token_id): Path<Uuid>,
    Json(req): Json<ExtendSessionRequest>,
) -> Result<Json<ExtendSessionResponse>, ApiError> {
    let requested = req.extend_hours.unwrap_or_else(|| {
        if auth.role == 0 {
            DEFAULT_PATIENT_TTL_HOURS
        } else {
            state.session_ttl_hours
        }
    });

    if requested <= 0 {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "extend_hours must be positive".into(),
        ));
    }
    if requested > MAX_EXTEND_HOURS {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            format!("extend_hours too large (max {MAX_EXTEND_HOURS})"),
        ));
    }

    // owner can extend own; admin/manager can extend any
    let bind_user = !(auth.role == 1 || auth.role == 2);

    // We compute: new_expires = GREATEST(expires_at, now()) + requested hours
    // but cap it to now + MAX_EXTEND_HOURS to avoid infinite growth.
    let sql = if bind_user {
        r#"
        UPDATE session_token
        SET expires_at = LEAST(
              GREATEST(expires_at, now()) + make_interval(hours => $3),
              now() + make_interval(hours => $4)
            )
        WHERE session_token_id = $1
          AND user_id = $2
          AND revoked_at IS NULL
        RETURNING expires_at
        "#
    } else {
        r#"
        UPDATE session_token
        SET expires_at = LEAST(
              GREATEST(expires_at, now()) + make_interval(hours => $2),
              now() + make_interval(hours => $3)
            )
        WHERE session_token_id = $1
          AND revoked_at IS NULL
        RETURNING expires_at
        "#
    };

    let expires_row: Option<(chrono::DateTime<chrono::Utc>,)> = if bind_user {
        sqlx::query_as(sql)
            .bind(session_token_id)
            .bind(auth.user_id)
            .bind(requested)
            .bind(MAX_EXTEND_HOURS)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    } else {
        sqlx::query_as(sql)
            .bind(session_token_id)
            .bind(requested)
            .bind(MAX_EXTEND_HOURS)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    };

    let expires_at = expires_row
        .ok_or_else(|| ApiError::BadRequest("NOT_FOUND", "session not found, revoked, or not allowed".into()))?
        .0;

    Ok(Json(ExtendSessionResponse {
        data: ExtendSessionData {
            ok: true,
            session_token_id,
            expires_at,
        },
    }))
}

#[derive(Debug, Serialize)]
pub struct RevokeOneResponse {
    pub data: RevokeOneData,
}

#[derive(Debug, Serialize)]
pub struct RevokeOneData {
    pub ok: bool,
    pub revoked_session_token_id: Uuid,
}

pub async fn revoke_session(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(session_token_id): Path<Uuid>,
) -> Result<Json<RevokeOneResponse>, ApiError> {
    // Revoke only your own session (admin override can be added later)
    let res = sqlx::query(
        r#"
        UPDATE session_token
        SET revoked_at = now()
        WHERE session_token_id = $1
          AND user_id = $2
          AND revoked_at IS NULL
        "#,
    )
    .bind(session_token_id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    if res.rows_affected() == 0 {
        return Err(ApiError::BadRequest(
            "NOT_FOUND",
            "session not found, already revoked, or not yours".into(),
        ));
    }

    Ok(Json(RevokeOneResponse {
        data: RevokeOneData {
            ok: true,
            revoked_session_token_id: session_token_id,
        },
    }))
}

#[derive(Debug, Serialize)]
pub struct RevokeAllResponse {
    pub data: RevokeAllData,
}

#[derive(Debug, Serialize)]
pub struct RevokeAllData {
    pub ok: bool,
    pub revoked_count: i64,
}

pub async fn revoke_all_sessions(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<RevokeAllResponse>, ApiError> {
    // Revoke everything except current session (and only active ones)
    let res = sqlx::query(
        r#"
        UPDATE session_token
        SET revoked_at = now()
        WHERE user_id = $1
          AND revoked_at IS NULL
          AND expires_at > now()
          AND session_token_id <> $2
        "#,
    )
    .bind(auth.user_id)
    .bind(auth.session_token_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(RevokeAllResponse {
        data: RevokeAllData {
            ok: true,
            revoked_count: res.rows_affected() as i64,
        },
    }))
}

// =========================
// Admin-only: impersonation
// =========================

#[derive(Debug, Serialize)]
pub struct ImpersonateResponse {
    pub data: ImpersonateData,
}

#[derive(Debug, Serialize)]
pub struct ImpersonateData {
    pub access_token: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub dcms_user: UserProfile,
    pub clinic: ClinicProfile,
}

/// POST /api/v1/auth/impersonate/{user_id}
/// Creates a new session as the target user (admin-only).
///
/// Requires DB migration that adds these nullable columns to `session_token`:
/// - impersonator_user_id UUID NULL
/// - impersonated_user_id UUID NULL
pub async fn impersonate(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(target_user_id): Path<Uuid>,
) -> Result<Json<ImpersonateResponse>, ApiError> {
    ensure_admin(&auth)?;

    // Load target user
    let target: UserRow = sqlx::query_as::<_, UserRow>(
        r#"
        SELECT user_id, username, display_name, password_hash, roles, is_active
        FROM "dcms_user"
        WHERE user_id = $1
        "#,
    )
    .bind(target_user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    .ok_or_else(|| ApiError::BadRequest("NOT_FOUND", "target user not found".into()))?;

    if !target.is_active {
        return Err(ApiError::BadRequest(
            "NOT_FOUND",
            "target user is disabled".into(),
        ));
    }

    // Load clinic name (singleton)
    let clinic_name = load_clinic_name(&state).await?;

    // Create new session as target user
    let access_token = generate_access_token();
    let token_hash = hash_access_token(&access_token);

    // Impersonation sessions should be short-lived by default.
    let expires_at = Utc::now() + Duration::hours(2);

    let _session: SessionTokenRow = sqlx::query_as::<_, SessionTokenRow>(
        r#"
        INSERT INTO session_token
            (user_id, session_token_hash, session_type, device_name, expires_at,
             impersonator_user_id, impersonated_user_id)
        VALUES
            ($1, $2, $3, $4, $5, $6, $7)
        RETURNING session_token_id, user_id, expires_at
        "#,
    )
    .bind(target.user_id)
    .bind(&token_hash)
    .bind(SESSION_TYPE_USER_PORTAL)
    .bind(Some(format!("Impersonated by {}", auth.user_id)))
    .bind(expires_at)
    .bind(auth.user_id)
    .bind(target.user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(ImpersonateResponse {
        data: ImpersonateData {
            access_token,
            expires_at: _session.expires_at,
            dcms_user: UserProfile {
                user_id: target.user_id,
                username: target.username,
                display_name: target.display_name,
                roles: vec![role_to_string(target.roles)],
            },
            clinic: ClinicProfile { clinic_name },
        },
    }))
}

// =========================
// Password management
// =========================

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub old_password: String,
    pub new_password: String,
}

#[derive(Debug, Serialize)]
pub struct ChangePasswordResponse {
    pub data: OkData,
}

fn validate_new_password(pw: &str) -> Result<(), ApiError> {
    let pw = pw.trim();
    if pw.len() < 8 {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "new_password must be at least 8 characters".into(),
        ));
    }
    Ok(())
}

pub async fn change_password(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<Json<ChangePasswordResponse>, ApiError> {
    if req.old_password.is_empty() || req.new_password.is_empty() {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "old_password and new_password are required".into(),
        ));
    }
    validate_new_password(&req.new_password)?;

    // Load current hash
    let row: (String,) = sqlx::query_as(
        r#"
        SELECT password_hash
        FROM "dcms_user"
        WHERE user_id = $1
          AND is_active = true
        "#,
    )
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    .ok_or_else(ApiError::session_expired)?;

    // Verify old password
    if !verify_password(&req.old_password, &row.0) {
        // Use invalid_credentials to avoid leaking info
        return Err(ApiError::invalid_credentials());
    }

    // Hash + update
    let new_hash = hash_password(&req.new_password)
        .map_err(|e| ApiError::Internal(e))?;

    // Do in a transaction so we can revoke sessions consistently
    let mut tx = state.db.begin().await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    sqlx::query(
        r#"
        UPDATE "dcms_user"
        SET password_hash = $1
        WHERE user_id = $2
        "#,
    )
    .bind(new_hash)
    .bind(auth.user_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    // Security: revoke all OTHER active sessions (keep current)
    sqlx::query(
        r#"
        UPDATE session_token
        SET revoked_at = now()
        WHERE user_id = $1
          AND revoked_at IS NULL
          AND expires_at > now()
          AND session_token_id <> $2
        "#,
    )
    .bind(auth.user_id)
    .bind(auth.session_token_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    tx.commit().await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(ChangePasswordResponse {
        data: OkData { ok: true },
    }))
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    /// Choose one identifier style; easiest is username.
    pub username: String,

    /// If omitted, backend generates a temporary password and returns it.
    pub new_password: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ResetPasswordResponse {
    pub data: ResetPasswordData,
}

#[derive(Debug, Serialize)]
pub struct ResetPasswordData {
    pub ok: bool,
    pub user_id: Uuid,
    pub username: String,
    pub temporary_password: Option<String>,
}

fn ensure_admin_or_manager(auth: &AuthContext) -> Result<(), ApiError> {
    // roles: 1 admin, 2 manager
    if auth.role == 1 || auth.role == 2 {
        Ok(())
    } else {
        Err(ApiError::Forbidden(
            "FORBIDDEN",
            "Only admin/manager can reset passwords".into(),
        ))
    }
}

fn ensure_admin(auth: &AuthContext) -> Result<(), ApiError> {
    if auth.role == 1 {
        Ok(())
    } else {
        Err(ApiError::Forbidden(
            "FORBIDDEN",
            "Only admin can perform this action".into(),
        ))
    }
}

fn generate_temp_password() -> String {
    // Use existing secure RNG + URL-safe encoding then trim to something copyable.
    // 16-24 chars is usually enough for a temp password in dev.
    crate::auth::generate_access_token().chars().take(20).collect()
}


pub async fn reset_password(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(req): Json<ResetPasswordRequest>,
) -> Result<Json<ResetPasswordResponse>, ApiError> {
    ensure_admin_or_manager(&auth)?;

    let username = req.username.trim();
    if username.is_empty() {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "username is required".into(),
        ));
    }

    let (new_pw, return_pw) = match req.new_password.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(pw) => {
            validate_new_password(pw)?;
            (pw.to_string(), None)
        }
        None => {
            let temp = generate_temp_password();
            // temp is long enough; still validate for consistency
            validate_new_password(&temp)?;
            (temp.clone(), Some(temp))
        }
    };

    let new_hash = hash_password(&new_pw)
        .map_err(|e| ApiError::Internal(e))?;

    let mut tx = state.db.begin().await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    // Find target user
    let target: (Uuid, String) = sqlx::query_as(
        r#"
        SELECT user_id, username
        FROM "dcms_user"
        WHERE username = $1
        "#,
    )
    .bind(username)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    .ok_or_else(|| ApiError::BadRequest("NOT_FOUND", "user not found".into()))?;

    // Update password hash
    sqlx::query(
        r#"
        UPDATE "dcms_user"
        SET password_hash = $1
        WHERE user_id = $2
        "#,
    )
    .bind(new_hash)
    .bind(target.0)
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    // Security: revoke ALL active sessions for that user
    sqlx::query(
        r#"
        UPDATE session_token
        SET revoked_at = now()
        WHERE user_id = $1
          AND revoked_at IS NULL
        "#,
    )
    .bind(target.0)
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    tx.commit().await
        .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(ResetPasswordResponse {
        data: ResetPasswordData {
            ok: true,
            user_id: target.0,
            username: target.1,
            temporary_password: return_pw,
        },
    }))
}
