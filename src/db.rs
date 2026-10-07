use crate::{config::Config, models::Settings};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::time::Duration;

pub async fn connect(config: &Config) -> Result<PgPool, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(8))
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    sqlx::query("INSERT INTO settings (id,data) VALUES (TRUE,$1) ON CONFLICT DO NOTHING")
        .bind(sqlx::types::Json(Settings::default()))
        .execute(&pool)
        .await?;
    Ok(pool)
}

pub async fn settings(pool: &PgPool) -> Result<Settings, sqlx::Error> {
    let (json,): (sqlx::types::Json<Settings>,) =
        sqlx::query_as("SELECT data FROM settings WHERE id=TRUE")
            .fetch_one(pool)
            .await?;
    Ok(json.0)
}

pub async fn rate_limit(pool: &PgPool, bucket: &str, limit: i32) -> Result<bool, sqlx::Error> {
    let (attempts,): (i32,) = sqlx::query_as("INSERT INTO rate_limits(bucket) VALUES ($1) ON CONFLICT(bucket) DO UPDATE SET attempts=CASE WHEN rate_limits.window_start < NOW()-INTERVAL '1 minute' THEN 1 ELSE rate_limits.attempts+1 END, window_start=CASE WHEN rate_limits.window_start < NOW()-INTERVAL '1 minute' THEN NOW() ELSE rate_limits.window_start END RETURNING attempts")
        .bind(bucket).fetch_one(pool).await?;
    Ok(attempts <= limit)
}
