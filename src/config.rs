use crate::client_ip::ClientIpSource;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::env;
use url::Url;

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub password_hash: String,
    pub base_url: String,
    pub secure_cookie: bool,
    pub session_generation: String,
    pub client_ip: ClientIpSource,
    pub event_capacity: i64,
    pub total_capacity: i64,
    session_secret: String,
}

pub fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

impl Config {
    pub fn new(
        database_url: String,
        password: &str,
        session_secret: &str,
        base: &str,
    ) -> Result<Self, String> {
        if password.chars().count() < 16 || password.len() > 256 {
            return Err("ADMIN_PASSWORD must contain 16–256 characters.".into());
        }
        if session_secret.len() < 32 || session_secret.len() > 256 || session_secret == password {
            return Err("Set SESSION_SECRET to a separate random secret of 32–256 bytes.".into());
        }
        let parsed =
            Url::parse(base).map_err(|_| "APP_BASE_URL must be an absolute HTTP(S) origin.")?;
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
        let mut config = Self {
            database_url,
            password_hash: digest(password),
            base_url: parsed.origin().ascii_serialization(),
            secure_cookie: parsed.scheme() == "https",
            session_generation: String::new(),
            session_secret: session_secret.into(),
            client_ip: ClientIpSource::Direct,
            event_capacity: 1000,
            total_capacity: 10000,
        };
        config.session_generation =
            config.keyed_digest("host-session-generation", &config.password_hash);
        Ok(config)
    }

    pub fn keyed_digest(&self, purpose: &str, value: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.session_secret.as_bytes())
            .expect("HMAC accepts any key length");
        mac.update(purpose.as_bytes());
        mac.update(&[0]);
        mac.update(value.as_bytes());
        format!("{:x}", mac.finalize().into_bytes())
    }

    pub fn from_env() -> Result<Self, String> {
        let database_url = env::var("DATABASE_URL")
            .map_err(|_| "Set DATABASE_URL to a PostgreSQL connection string.")?;
        let password = env::var("ADMIN_PASSWORD")
            .map_err(|_| "Set ADMIN_PASSWORD to a unique password of at least 16 characters.")?;
        let session_secret = env::var("SESSION_SECRET")
            .map_err(|_| "Set SESSION_SECRET to a separate random secret of at least 32 bytes.")?;
        let base = env::var("APP_BASE_URL")
            .or_else(|_| env::var("RENDER_EXTERNAL_URL"))
            .or_else(|_| {
                env::var("VERCEL_PROJECT_PRODUCTION_URL")
                    .or_else(|_| env::var("VERCEL_URL"))
                    .map(|v| format!("https://{v}"))
            })
            .unwrap_or_else(|_| "http://localhost:3030".into());
        let mut config = Self::new(database_url, &password, &session_secret, &base)?;
        config.client_ip = ClientIpSource::from_env()?;
        for (name, value) in [
            ("MAX_VISITORS_PER_EVENT", &mut config.event_capacity),
            ("MAX_VISITORS_TOTAL", &mut config.total_capacity),
        ] {
            if let Ok(raw) = env::var(name) {
                *value = raw
                    .parse()
                    .map_err(|_| format!("{name} must be an integer from 1 to 1000000."))?;
                if !(1..=1_000_000).contains(value) {
                    return Err(format!("{name} must be from 1 to 1000000."));
                }
            }
        }
        Ok(config)
    }
}
