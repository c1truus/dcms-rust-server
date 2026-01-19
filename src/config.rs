use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: String,
    pub session_ttl_hours: i64,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = env::var("DATABASE_URL")?;
        let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
        let session_ttl_hours = env::var("SESSION_TTL_HOURS")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(24);

        Ok(Self {
            database_url,
            bind_addr,
            session_ttl_hours,
        })
    }
}
