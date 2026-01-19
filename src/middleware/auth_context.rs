use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::TypedHeader;
use headers::{Authorization, authorization::Bearer};
use uuid::Uuid;

use crate::auth::hash_access_token;
use crate::error::ApiError;
use crate::models::AppState;

#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user_id: Uuid,
    pub role: i16,
    pub session_token_id: Uuid,
}

#[derive(Debug, sqlx::FromRow)]
struct SessionLookupRow {
    session_token_id: Uuid,
    user_id: Uuid,
    roles: i16,
}

impl FromRequestParts<AppState> for AuthContext {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        async move {
            // Extract Authorization: Bearer <token>
            let TypedHeader(authz): TypedHeader<Authorization<Bearer>> =
                TypedHeader::from_request_parts(parts, state)
                    .await
                    .map_err(|_| ApiError::session_expired())?;

            let token_hash = hash_access_token(authz.token());

            // Validate session_token + ensure dcms_user is active
            let row: SessionLookupRow = sqlx::query_as::<_, SessionLookupRow>(
                r#"
                SELECT st.session_token_id, st.user_id, u.roles
                FROM session_token st
                JOIN "dcms_user" u ON u.user_id = st.user_id
                WHERE st.session_token_hash = $1
                  AND st.revoked_at IS NULL
                  AND st.expires_at > now()
                  AND u.is_active = true
                "#,
            )
            .bind(&token_hash)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiError::Internal(format!("db error: {e}")))?
            .ok_or_else(ApiError::session_expired)?;

            // Touch last_seen_at (best-effort)
            let _ = sqlx::query(
                r#"
                UPDATE session_token
                SET last_seen_at = now()
                WHERE session_token_id = $1
                "#,
            )
            .bind(row.session_token_id)
            .execute(&state.db)
            .await;

            Ok(AuthContext {
                user_id: row.user_id,
                role: row.roles,
                session_token_id: row.session_token_id,
            })
        }
    }
}
