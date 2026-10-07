use crate::{ApiError, AppState, config::digest, db};
use axum::{
    Json,
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::json;
use subtle::ConstantTimeEq;
use uuid::Uuid;

const COOKIE: &str = "houseworks_session";

fn token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix(&format!("{COOKIE}=")))
}
pub fn is_app_request(headers: &HeaderMap) -> bool {
    headers
        .get("x-houseworks-request")
        .and_then(|h| h.to_str().ok())
        == Some("1")
}
pub async fn require_host(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if request.method() != axum::http::Method::GET && !is_app_request(request.headers()) {
        return ApiError::new(StatusCode::FORBIDDEN, "Use the app to make this change.")
            .into_response();
    }
    let Some(token) = token(request.headers()).filter(|t| t.len() == 64) else {
        return ApiError::unauthorized().into_response();
    };
    let active: Result<(bool,), _> = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM sessions WHERE token_hash=$1 AND password_fingerprint=$2 AND expires_at>NOW())")
        .bind(digest(token)).bind(&state.config.password_hash).fetch_one(&state.pool).await;
    match active {
        Ok((true,)) => next.run(request).await,
        Ok((false,)) => ApiError::unauthorized().into_response(),
        Err(e) => ApiError::from(e).into_response(),
    }
}
#[derive(Deserialize)]
pub struct Login {
    password: String,
}
pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Login>,
) -> Result<Response, ApiError> {
    if !is_app_request(&headers) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "Use the sign-in form.",
        ));
    }
    if !db::rate_limit(&state.pool, "login", 30).await? {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many sign-in attempts. Please wait one minute.",
        ));
    }
    if input.password.len() > 256
        || digest(&input.password)
            .as_bytes()
            .ct_eq(state.config.password_hash.as_bytes())
            .unwrap_u8()
            != 1
    {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "That host password isn't correct.",
        ));
    }
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    sqlx::query("DELETE FROM sessions WHERE expires_at<NOW() OR password_fingerprint<>$1")
        .bind(&state.config.password_hash)
        .execute(&state.pool)
        .await?;
    sqlx::query(
        "INSERT INTO sessions(token_hash,password_fingerprint,expires_at) VALUES ($1,$2,$3)",
    )
    .bind(digest(&token))
    .bind(&state.config.password_hash)
    .bind(Utc::now() + Duration::hours(24))
    .execute(&state.pool)
    .await?;
    let secure = if state.config.secure_cookie {
        "; Secure"
    } else {
        ""
    };
    Ok((
        [(
            header::SET_COOKIE,
            format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=86400{secure}"),
        )],
        Json(json!({"ok":true})),
    )
        .into_response())
}
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if let Some(token) = token(&headers) {
        sqlx::query("DELETE FROM sessions WHERE token_hash=$1")
            .bind(digest(token))
            .execute(&state.pool)
            .await?;
    }
    Ok((
        [(
            header::SET_COOKIE,
            format!("{COOKIE}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0"),
        )],
        Json(json!({"ok":true})),
    )
        .into_response())
}

/// A shared device must drop its host session before presenting the kiosk.
/// This also succeeds for visitors who never had a host session.
pub async fn start_kiosk(state: State<AppState>, headers: HeaderMap) -> Result<Response, ApiError> {
    if !is_app_request(&headers) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "Use the kiosk launch control.",
        ));
    }
    logout(state, headers).await
}
