mod auth;
mod config;
mod middleware;

mod db;
mod error;
mod models;
mod routes;

use crate::{config::Config, models::AppState};

use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use axum::http::header;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let cfg = Config::from_env()?;
    let pool = db::connect_pg(&cfg.database_url).await?;

    let state = AppState {
        db: pool,
        session_ttl_hours: cfg.session_ttl_hours,
    };

    // DEV ONLY: allow browser/WebView clients (Tauri static frontend) to call the API.
    // This fixes OPTIONS preflight (CORS) that otherwise returns 405 and blocks POST /auth/login.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::ACCEPT,
        ]);

    let app = routes::router(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    tracing::info!("Listening on http://{}", cfg.bind_addr);
    let listener = tokio::net::TcpListener::bind(&cfg.bind_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

