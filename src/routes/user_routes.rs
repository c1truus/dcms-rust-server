// src/routes/user_routes.rs

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::hash_password,
    error::ApiError,
    middleware::auth_context::AuthContext,
    models::AppState,
};

fn ensure_admin_or_manager(auth: &AuthContext) -> Result<(), ApiError> {
    // roles: 1 admin, 2 manager
    if auth.role == 1 || auth.role == 2 {
        Ok(())
    } else {
        Err(ApiError::Forbidden(
            "FORBIDDEN",
            "Only admin/manager can manage users".into(),
        ))
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct UserPublicRow {
    pub user_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub roles: i16,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct UsersListResponse {
    pub data: UsersListData,
}

#[derive(Debug, Serialize)]
pub struct UsersListData {
    pub users: Vec<UserPublicRow>,
}

#[derive(Debug, Serialize)]
pub struct UserGetResponse {
    pub data: UserPublicRow,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub display_name: String,
    pub password: String,
    pub roles: i16,              // 0..4
    pub is_active: Option<bool>, // default true
}

#[derive(Debug, Serialize)]
pub struct CreateUserResponse {
    pub data: UserPublicRow,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub display_name: Option<String>,
    pub roles: Option<i16>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct UpdateUserResponse {
    pub data: UserPublicRow,
}

#[derive(Debug, Serialize)]
pub struct OkResponse {
    pub data: OkData,
}

#[derive(Debug, Serialize)]
pub struct OkData {
    pub ok: bool,
}

pub fn router() -> Router<AppState> {
    Router::new()
        // /api/v1/users
        .route("/", get(list_users).post(create_user))
        // /api/v1/users/{user_id}
        .route("/{user_id}", get(get_user).patch(update_user))
        // /api/v1/users/{user_id}/disable
        .route("/{user_id}/disable", post(disable_user))
        // /api/v1/users/{user_id}/enable
        .route("/{user_id}/enable", post(enable_user))
}

pub async fn list_users(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<UsersListResponse>, ApiError> {
    ensure_admin_or_manager(&auth)?;

    let users: Vec<UserPublicRow> = sqlx::query_as::<_, UserPublicRow>(
        r#"
        SELECT user_id, username, display_name, roles, is_active, created_at
        FROM "dcms_user"
        ORDER BY created_at DESC
        LIMIT 200
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(UsersListResponse {
        data: UsersListData { users },
    }))
}

pub async fn get_user(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(user_id): Path<Uuid>,
) -> Result<Json<UserGetResponse>, ApiError> {
    ensure_admin_or_manager(&auth)?;

    let user: UserPublicRow = sqlx::query_as::<_, UserPublicRow>(
        r#"
        SELECT user_id, username, display_name, roles, is_active, created_at
        FROM "dcms_user"
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    .ok_or_else(|| ApiError::BadRequest("NOT_FOUND", "user not found".into()))?;

    Ok(Json(UserGetResponse { data: user }))
}

fn validate_role(roles: i16) -> Result<(), ApiError> {
    if !(0..=4).contains(&roles) {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "roles must be one of 0..4".into(),
        ));
    }
    Ok(())
}

fn validate_username(username: &str) -> Result<(), ApiError> {
    let u = username.trim();
    if u.is_empty() {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "username is required".into(),
        ));
    }
    if u.len() < 3 {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "username must be at least 3 characters".into(),
        ));
    }
    Ok(())
}

fn validate_display_name(display_name: &str) -> Result<(), ApiError> {
    let d = display_name.trim();
    if d.is_empty() {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "display_name is required".into(),
        ));
    }
    Ok(())
}

fn validate_password(pw: &str) -> Result<(), ApiError> {
    let p = pw.trim();
    if p.len() < 8 {
        return Err(ApiError::BadRequest(
            "VALIDATION_ERROR",
            "password must be at least 8 characters".into(),
        ));
    }
    Ok(())
}

pub async fn create_user(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<CreateUserResponse>, ApiError> {
    ensure_admin_or_manager(&auth)?;

    validate_username(&req.username)?;
    validate_display_name(&req.display_name)?;
    validate_password(&req.password)?;
    validate_role(req.roles)?;

    let username = req.username.trim().to_string();
    let display_name = req.display_name.trim().to_string();
    let is_active = req.is_active.unwrap_or(true);

    let pw_hash = hash_password(req.password.trim())
        .map_err(|e| ApiError::Internal(e))?;

    // Insert
    let user: UserPublicRow = sqlx::query_as::<_, UserPublicRow>(
        r#"
        INSERT INTO "dcms_user" (username, display_name, password_hash, roles, is_active)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING user_id, username, display_name, roles, is_active, created_at
        "#,
    )
    .bind(&username)
    .bind(&display_name)
    .bind(&pw_hash)
    .bind(req.roles)
    .bind(is_active)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        // If you want better UX later, detect unique violation on username.
        ApiError::Internal(format!("db error: {e}"))
    })?;

    Ok(Json(CreateUserResponse { data: user }))
}

pub async fn update_user(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(user_id): Path<Uuid>,
    Json(req): Json<UpdateUserRequest>,
) -> Result<Json<UpdateUserResponse>, ApiError> {
    ensure_admin_or_manager(&auth)?;

    // Load existing
    let existing: UserPublicRow = sqlx::query_as::<_, UserPublicRow>(
        r#"
        SELECT user_id, username, display_name, roles, is_active, created_at
        FROM "dcms_user"
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
    .ok_or_else(|| ApiError::BadRequest("NOT_FOUND", "user not found".into()))?;

    // Compute updates
    let display_name = match req.display_name.as_deref().map(str::trim) {
        Some(s) if !s.is_empty() => {
            validate_display_name(s)?;
            s.to_string()
        }
        _ => existing.display_name.clone(),
    };

    let roles = match req.roles {
        Some(r) => {
            validate_role(r)?;
            r
        }
        None => existing.roles,
    };

    let is_active = req.is_active.unwrap_or(existing.is_active);

    // Apply
    let updated: UserPublicRow = sqlx::query_as::<_, UserPublicRow>(
        r#"
        UPDATE "dcms_user"
        SET display_name = $1,
            roles = $2,
            is_active = $3
        WHERE user_id = $4
        RETURNING user_id, username, display_name, roles, is_active, created_at
        "#,
    )
    .bind(&display_name)
    .bind(roles)
    .bind(is_active)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(UpdateUserResponse { data: updated }))
}

pub async fn disable_user(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(user_id): Path<Uuid>,
) -> Result<Json<OkResponse>, ApiError> {
    ensure_admin_or_manager(&auth)?;

    let res = sqlx::query(
        r#"
        UPDATE "dcms_user"
        SET is_active = false
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    if res.rows_affected() == 0 {
        return Err(ApiError::BadRequest("NOT_FOUND", "user not found".into()));
    }

    Ok(Json(OkResponse {
        data: OkData { ok: true },
    }))
}

pub async fn enable_user(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(user_id): Path<Uuid>,
) -> Result<Json<OkResponse>, ApiError> {
    ensure_admin_or_manager(&auth)?;

    let res = sqlx::query(
        r#"
        UPDATE "dcms_user"
        SET is_active = true
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    if res.rows_affected() == 0 {
        return Err(ApiError::BadRequest("NOT_FOUND", "user not found".into()));
    }

    Ok(Json(OkResponse {
        data: OkData { ok: true },
    }))
}


// In src/routes/user_routes.rs (at the bottom)
#[cfg(test)]
mod tests {
    use super::*;
    // use crate::error::ApiError;
    
    #[test]
    fn test_validate_role_bounds() {
        // Valid roles should pass
        assert!(validate_role(0).is_ok());
        assert!(validate_role(2).is_ok());
        assert!(validate_role(4).is_ok());
        
        // Invalid roles should fail
        assert!(validate_role(-1).is_err());
        assert!(validate_role(5).is_err());
        assert!(validate_role(100).is_err());
    }
    
    #[test]
    fn test_validate_username() {
        assert!(validate_username("alice").is_ok());
        assert!(validate_username("al").is_err()); // Too short
        assert!(validate_username("").is_err());
        assert!(validate_username("  ").is_err()); // Only whitespace
    }
    
    #[test]
    fn test_validate_password() {
        assert!(validate_password("password123").is_ok());
        assert!(validate_password("short").is_err()); // Too short
        assert!(validate_password("").is_err());
    }
}