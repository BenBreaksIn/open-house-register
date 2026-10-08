pub mod api;
pub mod auth;
pub mod client_ip;
pub mod config;
pub mod db;
pub mod models;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Request},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{delete, get, post, put},
};
use serde_json::json;
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: config::Config,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}
impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }
    pub fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "Please sign in to your host workspace.",
        )
    }
    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "This page could not be found.")
    }
    pub fn internal(_error: impl std::fmt::Display) -> Self {
        eprintln!("A storage or response operation failed.");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Something went wrong. Please try again.",
        )
    }
}
impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        Self::internal(e)
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({"error":self.message}))).into_response()
    }
}

async fn page() -> Html<&'static str> {
    Html(include_str!("../web/index.html"))
}
async fn security_headers(request: Request, next: Next) -> Response {
    let mut res = next.run(request).await;
    let h = res.headers_mut();
    h.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    h.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    h.insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    h.insert(header::CONTENT_SECURITY_POLICY,"default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' https:; connect-src 'self'; font-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'".parse().unwrap());
    res
}
pub fn router(state: AppState) -> Router {
    let admin = Router::new()
        .route("/api/admin/dashboard", get(api::dashboard))
        .route("/api/admin/settings", put(api::save_settings))
        .route("/api/admin/houses", post(api::create_house))
        .route("/api/admin/houses/{id}", put(api::update_house))
        .route("/api/admin/houses/{id}/export", get(api::export))
        .route("/api/admin/visitors/{id}", delete(api::delete_visitor))
        .route("/api/logout", post(auth::logout))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_host,
        ));
    Router::new()
        .merge(admin)
        .route("/api/login", post(auth::login))
        .route("/api/kiosk/start", post(auth::start_kiosk))
        .route(
            "/api/public/houses/{id}",
            get(api::guest_info).post(api::register),
        )
        .route("/api/public/houses/{id}/qr.svg", get(api::qr))
        .route("/health", get(api::health))
        .route(
            "/assets/icon.svg",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "image/svg+xml")],
                    include_str!("../web/icon.svg"),
                )
            }),
        )
        .route(
            "/assets/app.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
                    include_str!("../web/app.css"),
                )
            }),
        )
        .route(
            "/assets/app.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../web/app.js"),
                )
            }),
        )
        .route(
            "/assets/kiosk.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../web/kiosk.js"),
                )
            }),
        )
        .route(
            "/assets/kiosk-view.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../web/kiosk-view.js"),
                )
            }),
        )
        .route(
            "/assets/kiosk.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
                    include_str!("../web/kiosk.css"),
                )
            }),
        )
        .route(
            "/assets/sample-house.png",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "image/png")],
                    include_bytes!("../web/sample-house.png").as_slice(),
                )
            }),
        )
        .route("/", get(page))
        .route("/customize", get(page))
        .route("/visitors", get(page))
        .route("/visit/{id}", get(page))
        .route("/kiosk/{id}", get(page))
        .route("/sign/{id}", get(page))
        .fallback(|| async { ApiError::not_found() })
        .layer(DefaultBodyLimit::max(32 * 1024))
        .layer(middleware::from_fn(security_headers))
        .with_state(state)
}
