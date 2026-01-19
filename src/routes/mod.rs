use crate::models::AppState;
use axum::Router;

pub mod auth_routes;
pub mod home_routes;
pub mod patient_comm_routes;
pub mod service_routes;
pub mod patient_routes;
pub mod user_routes;
pub mod clinic_routes;
// pub mod report_routes; maybe later


pub fn router(state: AppState) -> Router {
    Router::new()
        .nest("/api/v1/auth", auth_routes::router())
        .nest("/api/v1/users", user_routes::router())
        .nest("/api/v1/services", service_routes::router())
        .nest("/api/v1", clinic_routes::router()) 
        .nest("/api/v1", patient_comm_routes::router())
        .nest("/api/v1", patient_routes::router())
        .merge(home_routes::router())
        .with_state(state)
}
