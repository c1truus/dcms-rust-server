// src/routes/service_routes.rs

use axum::{Json, Router, extract::State, routing::get};

use crate::{
    error::ApiError,
    middleware::auth_context::AuthContext,
    models::{AppState, ServiceCatalogRow},
};

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list_services))
}

pub async fn list_services(
    State(state): State<AppState>,
    _auth: AuthContext,
) -> Result<Json<Vec<ServiceCatalogRow>>, ApiError> {
    let rows: Vec<ServiceCatalogRow> = sqlx::query_as::<_, ServiceCatalogRow>(
        r#"
        SELECT
          service_id,
          service_type,
          display_number,
          display_name,
          default_duration_min,
          disclaimer,
          price_cents,
          is_active,
          created_at,
          updated_at
        FROM service_catalog
        WHERE is_active = true
        ORDER BY display_number ASC, service_type ASC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db error: {e}")))?;

    Ok(Json(rows))
}
