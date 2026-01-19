mod auth;
mod config;
mod middleware;

mod db;
mod error;
mod models;
mod routes;

// dotenvy::dotenv().ok();

use crate::{config::Config, models::AppState};
use tower_http::trace::TraceLayer;
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

    let app = routes::router(state).layer(TraceLayer::new_for_http());

    tracing::info!("Listening on http://{}", cfg.bind_addr);
    let listener = tokio::net::TcpListener::bind(&cfg.bind_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
