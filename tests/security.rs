use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use open_house_register::{
    AppState,
    config::{Config, digest},
    db, router,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const PASSWORD: &str = "security-integration-password";
const SECRET: &str = "independent-security-test-secret-only";

fn request(
    method: &str,
    path: &str,
    value: Value,
    source: &str,
    cookie: Option<&str>,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-houseworks-request", "1")
        // An attacker-supplied header must not override the transport address.
        .header("x-forwarded-for", Uuid::new_v4().to_string())
        .extension(ConnectInfo(std::net::SocketAddr::new(
            source.parse().unwrap(),
            1234,
        )));
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    builder.body(Body::from(value.to_string())).unwrap()
}

async fn send(
    app: &Router,
    method: &str,
    path: &str,
    value: Value,
    source: &str,
    cookie: Option<&str>,
) -> axum::response::Response {
    app.clone()
        .oneshot(request(method, path, value, source, cookie))
        .await
        .unwrap()
}

async fn login(app: &Router, source: &str) -> String {
    let res = send(
        app,
        "POST",
        "/api/login",
        json!({"password":PASSWORD}),
        source,
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    res.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

async fn house(app: &Router, cookie: &str) -> Uuid {
    let res = send(app, "POST", "/api/admin/houses", json!({"address":"Security fixture","location":"Test city","starts_at":"2026-10-17T20:00:00Z","ends_at":"2026-10-17T23:00:00Z","timezone":"UTC","status":"open"}), "192.0.2.250", Some(cookie)).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    Uuid::parse_str(body["id"].as_str().unwrap()).unwrap()
}

async fn register(app: &Router, id: Uuid, email: &str, source: &str) -> StatusCode {
    send(
        app,
        "POST",
        &format!("/api/public/houses/{id}"),
        json!({"name":"Security fixture","email":email,"follow_up":false}),
        source,
        None,
    )
    .await
    .status()
}

async fn clear_limits(pool: &PgPool) {
    sqlx::query("TRUNCATE rate_limits")
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL TEST_DATABASE_URL"]
async fn security_boundaries_and_concurrent_capacity() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must point to a test database");
    let admin = PgPool::connect(&database_url).await.unwrap();
    let schema = format!("security_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await
        .unwrap();
    let mut url = url::Url::parse(&database_url).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-csearch_path={schema}"));
    let mut config = Config::new(
        url.to_string(),
        PASSWORD,
        SECRET,
        "https://openhouse.example.com",
    )
    .unwrap();
    // Simulate upgrading an existing database containing a fast password verifier.
    let old = PgPool::connect(&config.database_url).await.unwrap();
    let current: (String,) = sqlx::query_as("SELECT current_schema()")
        .fetch_one(&old)
        .await
        .unwrap();
    assert_eq!(current.0, schema);
    sqlx::raw_sql(include_str!("../migrations/0001_initial.sql"))
        .execute(&old)
        .await
        .unwrap();
    sqlx::query("INSERT INTO sessions VALUES ('legacy', $1, NOW()+INTERVAL '1 day')")
        .bind(digest(PASSWORD))
        .execute(&old)
        .await
        .unwrap();
    old.close().await;
    let pool = db::connect(&config).await.unwrap();
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 0, "upgrade must discard old password fingerprints");
    let legacy: (bool,) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=current_schema() AND table_name='sessions' AND column_name='password_fingerprint')").fetch_one(&pool).await.unwrap();
    assert!(!legacy.0);
    let app = router(AppState {
        pool: pool.clone(),
        config: config.clone(),
    });

    // One source cannot lock out other clients, even by rotating spoofed headers.
    for _ in 0..10 {
        assert_eq!(
            send(
                &app,
                "POST",
                "/api/login",
                json!({"password":"wrong"}),
                "192.0.2.1",
                None
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/login",
            json!({"password":PASSWORD}),
            "192.0.2.1",
            None
        )
        .await
        .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/login",
            json!({"password":PASSWORD}),
            "::ffff:192.0.2.1",
            None
        )
        .await
        .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    let cookie = login(&app, "192.0.2.2").await;
    let global: (i32,) =
        sqlx::query_as("SELECT attempts FROM rate_limits WHERE bucket='login:global'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        global.0, 11,
        "blocked clients must not consume the global budget"
    );
    let generation: (String,) = sqlx::query_as("SELECT session_generation FROM sessions LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(generation.0, digest(PASSWORD));
    assert_eq!(generation.0, config.session_generation);
    for (password, secret) in [
        ("a-different-host-password", SECRET),
        (PASSWORD, "a-different-independent-session-secret"),
    ] {
        let rotated = Config::new(
            config.database_url.clone(),
            password,
            secret,
            &config.base_url,
        )
        .unwrap();
        assert_ne!(rotated.session_generation, generation.0);
        let other = router(AppState {
            pool: pool.clone(),
            config: rotated,
        });
        assert_eq!(
            send(
                &other,
                "GET",
                "/api/admin/dashboard",
                json!(null),
                "192.0.2.2",
                Some(&cookie)
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    let other = router(AppState {
        pool: pool.clone(),
        config: config.clone(),
    });
    assert_eq!(
        send(
            &other,
            "GET",
            "/api/admin/dashboard",
            json!(null),
            "192.0.2.2",
            Some(&cookie)
        )
        .await
        .status(),
        StatusCode::OK
    );

    let id = house(&app, &cookie).await;
    for i in 0..30 {
        assert_eq!(
            register(&app, id, &format!("a{i}@example.com"), "192.0.2.10").await,
            StatusCode::OK
        );
    }
    assert_eq!(
        register(&app, id, "blocked@example.com", "192.0.2.10").await,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        register(&app, id, "independent@example.com", "192.0.2.11").await,
        StatusCode::OK
    );
    // Rotating addresses within an IPv6 /64 cannot reset a client's budget.
    for i in 1..=30 {
        assert_eq!(
            register(
                &app,
                id,
                &format!("ipv6-{i}@example.com"),
                &format!("2001:db8:1:2::{i:x}")
            )
            .await,
            StatusCode::OK
        );
    }
    assert_eq!(
        register(&app, id, "ipv6-blocked@example.com", "2001:db8:1:2::ffff").await,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        register(&app, id, "ipv6-other@example.com", "2001:db8:1:3::1").await,
        StatusCode::OK
    );

    sqlx::query("TRUNCATE visitors")
        .execute(&pool)
        .await
        .unwrap();
    clear_limits(&pool).await;
    config.event_capacity = 3;
    config.total_capacity = 5;
    let limited = router(AppState {
        pool: pool.clone(),
        config: config.clone(),
    });
    let id2 = house(&limited, &cookie).await;
    let attempts = (0..12).map(|i| {
        let app = limited.clone();
        async move {
            register(
                &app,
                id,
                &format!("race{i}@example.com"),
                &format!("198.51.100.{}", i + 1),
            )
            .await
        }
    });
    let results = futures_util::future::join_all(attempts).await;
    assert_eq!(results.iter().filter(|s| **s == StatusCode::OK).count(), 3);
    assert_eq!(
        results
            .iter()
            .filter(|s| **s == StatusCode::CONFLICT)
            .count(),
        9
    );
    let saved: (String,) = sqlx::query_as("SELECT email FROM visitors WHERE house_id=$1 LIMIT 1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        register(&limited, id, &saved.0, "192.0.2.55").await,
        StatusCode::OK,
        "duplicates remain idempotent at capacity"
    );
    let results = futures_util::future::join_all((0..8).map(|i| {
        let app = limited.clone();
        async move {
            register(
                &app,
                id2,
                &format!("total{i}@example.com"),
                &format!("203.0.113.{}", i + 1),
            )
            .await
        }
    }))
    .await;
    assert_eq!(results.iter().filter(|s| **s == StatusCode::OK).count(), 2);
    assert_eq!(
        results
            .iter()
            .filter(|s| **s == StatusCode::CONFLICT)
            .count(),
        6
    );
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM visitors")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 5);
    let visitor: (Uuid,) = sqlx::query_as("SELECT id FROM visitors WHERE house_id=$1 LIMIT 1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        send(
            &limited,
            "DELETE",
            &format!("/api/admin/visitors/{}", visitor.0),
            json!(null),
            "192.0.2.2",
            Some(&cookie)
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        register(&limited, id, "after-delete@example.com", "192.0.2.56").await,
        StatusCode::OK
    );
    let res = send(
        &limited,
        "GET",
        "/api/admin/dashboard",
        json!(null),
        "192.0.2.2",
        Some(&cookie),
    )
    .await;
    let dashboard: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(dashboard["capacity"]["total"], 5);
    assert_eq!(dashboard["capacity"]["events"][id.to_string()], 3);

    // A multi-batch export with identical timestamps must neither skip nor repeat rows.
    sqlx::query("TRUNCATE visitors")
        .execute(&pool)
        .await
        .unwrap();
    for i in 0..450 {
        sqlx::query("INSERT INTO visitors(id,house_id,name,email,consent_text,checked_in_at) VALUES ($1,$2,$3,$4,'Permission, including a comma','2026-10-07T20:00:00Z')")
            .bind(Uuid::new_v4()).bind(id).bind("=SUM(1,1)\n\"Quoted\"").bind(format!("export{i}@example.com")).execute(&pool).await.unwrap();
    }
    let export = format!("/api/admin/houses/{id}/export");
    assert_eq!(
        send(&app, "GET", &export, json!(null), "192.0.2.2", None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let res = send(
        &app,
        "GET",
        &export,
        json!(null),
        "192.0.2.2",
        Some(&cookie),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let mut body = res.into_body();
    let mut bytes = Vec::new();
    let mut frames = 0;
    while let Some(frame) = body.frame().await {
        let chunk = frame.unwrap().into_data().unwrap();
        let rows = csv::ReaderBuilder::new()
            .has_headers(frames == 0)
            .from_reader(chunk.as_ref())
            .records()
            .count();
        assert!(rows <= 200);
        frames += 1;
        bytes.extend_from_slice(&chunk);
    }
    assert_eq!(frames, 3);
    let records = csv::Reader::from_reader(bytes.as_slice())
        .records()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(records.len(), 450);
    assert_eq!(
        records
            .iter()
            .map(|r| r[1].to_owned())
            .collect::<std::collections::HashSet<_>>()
            .len(),
        450
    );
    assert!(records.iter().all(|r| &r[0] == "'=SUM(1,1)\n\"Quoted\""));
    drop((app, other, limited));
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}
