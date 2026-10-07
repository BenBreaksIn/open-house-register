use sha2::{Digest, Sha256};
use std::env;
use url::Url;

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub password_hash: String,
    pub base_url: String,
    pub secure_cookie: bool,
}

pub fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let database_url = env::var("DATABASE_URL")
            .map_err(|_| "Set DATABASE_URL to a PostgreSQL connection string.")?;
        let password = env::var("ADMIN_PASSWORD")
            .map_err(|_| "Set ADMIN_PASSWORD to a unique password of at least 16 characters.")?;
        if password.chars().count() < 16 || password.len() > 256 {
            return Err("ADMIN_PASSWORD must contain 16–256 characters.".into());
        }
        let base = env::var("APP_BASE_URL")
            .or_else(|_| env::var("RENDER_EXTERNAL_URL"))
            .or_else(|_| {
                env::var("VERCEL_PROJECT_PRODUCTION_URL")
                    .or_else(|_| env::var("VERCEL_URL"))
                    .map(|v| format!("https://{v}"))
            })
            .unwrap_or_else(|_| "http://localhost:3030".into());
        let parsed =
            Url::parse(&base).map_err(|_| "APP_BASE_URL must be an absolute HTTP(S) origin.")?;
        let local = matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if !matches!(parsed.scheme(), "http" | "https")
            || (!local && parsed.scheme() != "https")
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || parsed.path() != "/"
        {
            return Err(
                "APP_BASE_URL must be an HTTPS origin (HTTP is allowed for localhost).".into(),
            );
        }
        Ok(Self {
            database_url,
            password_hash: digest(&password),
            base_url: parsed.origin().ascii_serialization(),
            secure_cookie: parsed.scheme() == "https",
        })
    }
}
