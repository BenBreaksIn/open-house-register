use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use open_house_register::{AppState, config::Config, db, models::Settings, router};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

async fn request(
    app: &Router,
    method: &str,
    path: &str,
    body: Value,
    cookie: Option<&str>,
    csrf: bool,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let mut req = Request::builder()
        .extension(ConnectInfo(
            "127.0.0.1:1234".parse::<std::net::SocketAddr>().unwrap(),
        ))
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    if csrf {
        req = req.header("x-houseworks-request", "1");
    }
    let res = app
        .clone()
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, headers, bytes)
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL TEST_DATABASE_URL"]
async fn persisted_registration_access_and_consent() {
    let config = Config::new(
        std::env::var("TEST_DATABASE_URL")
            .expect("TEST_DATABASE_URL must point to a test database"),
        "integration-test-password",
        "independent-registration-test-secret-only",
        "https://openhouse.example.com",
    )
    .unwrap();
    let pool = db::connect(&config).await.unwrap();
    let app = router(AppState {
        pool: pool.clone(),
        config,
    });
    let (s, _, _) = request(
        &app,
        "GET",
        "/api/admin/dashboard",
        json!(null),
        None,
        false,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    // Entering a shared-device kiosk revokes the host session server-side.
    let (_, headers, _) = request(
        &app,
        "POST",
        "/api/login",
        json!({"password":"integration-test-password"}),
        None,
        true,
    )
    .await;
    let kiosk_cookie = headers
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let (s, _, _) = request(
        &app,
        "POST",
        "/api/kiosk/start",
        json!({}),
        Some(kiosk_cookie),
        false,
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _, _) = request(
        &app,
        "GET",
        "/api/admin/dashboard",
        json!(null),
        Some(kiosk_cookie),
        false,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, headers, _) = request(
        &app,
        "POST",
        "/api/kiosk/start",
        json!({}),
        Some(kiosk_cookie),
        true,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(
        headers
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    let (s, _, _) = request(
        &app,
        "GET",
        "/api/admin/dashboard",
        json!(null),
        Some(kiosk_cookie),
        false,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _, _) = request(&app, "POST", "/api/kiosk/start", json!({}), None, true).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _, _) = request(
        &app,
        "GET",
        &format!("/kiosk/{}", Uuid::new_v4()),
        json!(null),
        None,
        false,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _, _) = request(
        &app,
        "POST",
        "/api/login",
        json!({"password":"integration-test-password"}),
        None,
        false,
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _, _) = request(
        &app,
        "POST",
        "/api/login",
        json!({"password":"wrong"}),
        None,
        true,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, headers, _) = request(
        &app,
        "POST",
        "/api/login",
        json!({"password":"integration-test-password"}),
        None,
        true,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let set_cookie = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
    assert!(
        set_cookie.contains("HttpOnly")
            && set_cookie.contains("Secure")
            && set_cookie.contains("SameSite=Strict")
    );
    let cookie = set_cookie.split(';').next().unwrap();
    let settings = Settings {
        business_name: "Test Brokerage".into(),
        agent_license: "DEMO-123".into(),
        broker_license: "DEMO-456".into(),
        agent_phone: "555-0100".into(),
        broker_email: "broker@example.com".into(),
        show_location: true,
        agent_photo_url: "https://example.com/agent.png".into(),
        logo_url: "https://example.com/brokerage.png".into(),
        ..Settings::default()
    };
    let (s, _, _) = request(
        &app,
        "PUT",
        "/api/admin/settings",
        json!(settings),
        Some(cookie),
        false,
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _, _) = request(
        &app,
        "PUT",
        "/api/admin/settings",
        json!(settings),
        Some(cookie),
        true,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let house = json!({"address":"Test Open House","location":"Oakland, CA","starts_at":"2026-10-17T20:00:00Z","ends_at":"2026-10-17T23:00:00Z","timezone":"America/Los_Angeles","status":"draft"});
    let (s, _, body) = request(
        &app,
        "POST",
        "/api/admin/houses",
        house.clone(),
        Some(cookie),
        true,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let h: Value = serde_json::from_slice(&body).unwrap();
    let id = h["id"].as_str().unwrap();
    let public = format!("/api/public/houses/{id}");
    let (s, _, _) = request(&app, "GET", &public, json!(null), None, false).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let mut open = house.clone();
    open["status"] = json!("open");
    let (s, _, _) = request(
        &app,
        "PUT",
        &format!("/api/admin/houses/{id}"),
        open.clone(),
        Some(cookie),
        true,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _, body) = request(&app, "GET", &public, json!(null), None, false).await;
    assert_eq!(s, StatusCode::OK);
    let guest: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(guest["settings"]["broker_license"], "DEMO-456");
    assert_eq!(guest["settings"]["show_location"], true);
    assert_eq!(
        guest["settings"]["agent_photo_url"],
        "https://example.com/agent.png"
    );
    assert_eq!(
        guest["settings"]["logo_url"],
        "https://example.com/brokerage.png"
    );
    assert!(guest.get("visitors").is_none());
    assert!(guest.get("password").is_none());
    let (s, _, body) = request(
        &app,
        "GET",
        &format!("{public}/qr.svg"),
        json!(null),
        None,
        false,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(String::from_utf8(body).unwrap().contains("<svg"));
    let email = format!("test-{}@example.com", Uuid::new_v4());
    let registration = json!({"name":"=SUM(1,1)","email":email,"phone":"555-0123","timeline":"0–3 months","represented":"No","follow_up":false});
    let (s, _, _) = request(&app, "POST", &public, registration.clone(), None, true).await;
    assert_eq!(s, StatusCode::OK);
    let mut duplicate = registration.clone();
    duplicate["follow_up"] = json!(true);
    duplicate["name"] = json!("Overwritten");
    request(&app, "POST", &public, duplicate, None, true).await;
    let record: (String, bool, String) = sqlx::query_as(
        "SELECT name,follow_up,consent_text FROM visitors WHERE house_id=$1 AND email=$2",
    )
    .bind(Uuid::parse_str(id).unwrap())
    .bind(&email)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(record.0, "=SUM(1,1)");
    assert!(!record.1);
    assert!(record.2.contains("follow up"));
    let (s, _, _) = request(
        &app,
        "GET",
        &format!("/api/admin/houses/{id}/export"),
        json!(null),
        None,
        false,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _, body) = request(
        &app,
        "GET",
        &format!("/api/admin/houses/{id}/export"),
        json!(null),
        Some(cookie),
        false,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let csv = String::from_utf8(body).unwrap();
    assert!(csv.contains("'=SUM(1,1)"));
    assert!(csv.contains("false"));
    let mut bad = registration.clone();
    bad["email"] = json!("not-an-email");
    let (s, _, _) = request(&app, "POST", &public, bad, None, true).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let mut trap = registration.clone();
    trap["email"] = json!("trap@example.com");
    trap["website"] = json!("spam");
    request(&app, "POST", &public, trap, None, true).await;
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM visitors WHERE house_id=$1")
        .bind(Uuid::parse_str(id).unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 1);
    open["status"] = json!("closed");
    request(
        &app,
        "PUT",
        &format!("/api/admin/houses/{id}"),
        open,
        Some(cookie),
        true,
    )
    .await;
    let (s, _, _) = request(&app, "POST", &public, registration, None, true).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _, _) = request(&app, "POST", "/api/logout", json!({}), Some(cookie), true).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _, _) = request(
        &app,
        "GET",
        "/api/admin/dashboard",
        json!(null),
        Some(cookie),
        false,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    sqlx::query("DELETE FROM open_houses WHERE id=$1")
        .bind(Uuid::parse_str(id).unwrap())
        .execute(&pool)
        .await
        .unwrap();
}
