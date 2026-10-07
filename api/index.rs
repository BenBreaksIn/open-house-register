use open_house_register::{AppState, config::Config, db, router};
use tower::{ServiceExt, service_fn};
use vercel_runtime::{Error, axum::StreamingUtils};

async fn adapt_response(
    response: axum::response::Response,
) -> Result<vercel_runtime::Response<vercel_runtime::ResponseBody>, Error> {
    let (parts, body) = response.into_parts();
    let body = StreamingUtils::create_stream_body(body).await?;
    Ok(vercel_runtime::Response::from_parts(parts, body))
}
#[tokio::main]
async fn main() -> Result<(), Error> {
    let config = Config::from_env()?;
    let pool = db::connect(&config).await?;
    let app = router(AppState { pool, config });
    // VercelLayer buffers non-JSON responses (including CSV). Preserve streaming
    // for every content type and let Axum enforce request-body limits directly.
    vercel_runtime::run(service_fn(
        move |(_runtime, request): (vercel_runtime::AppState, vercel_runtime::Request)| {
            let app = app.clone();
            async move {
                let response = app.oneshot(request.map(axum::body::Body::new)).await?;
                adapt_response(response).await
            }
        },
    ))
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::header};
    use futures_util::{StreamExt, stream};
    use http_body_util::BodyExt;

    #[tokio::test]
    async fn csv_adapter_yields_before_the_export_finishes() {
        let (release, wait) = tokio::sync::oneshot::channel();
        let chunks = stream::once(async {
            Ok::<_, std::io::Error>("Name,Email\nFirst,first@example.com\n")
        })
        .chain(stream::once(async move {
            wait.await.unwrap();
            Ok("Second,second@example.com\n")
        }));
        let response = axum::http::Response::builder()
            .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
            .header(
                header::CONTENT_DISPOSITION,
                "attachment; filename=visitors.csv",
            )
            .body(Body::from_stream(chunks))
            .unwrap();
        let response =
            tokio::time::timeout(std::time::Duration::from_secs(2), adapt_response(response))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(
            response.headers()[header::CONTENT_DISPOSITION],
            "attachment; filename=visitors.csv"
        );
        let mut body = response.into_body();
        let first = tokio::time::timeout(std::time::Duration::from_secs(2), body.frame())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .into_data()
            .unwrap();
        assert!(
            std::str::from_utf8(&first)
                .unwrap()
                .contains("first@example.com")
        );
        release.send(()).unwrap();
        let rest = body.collect().await.unwrap().to_bytes();
        assert_eq!(rest.as_ref(), b"Second,second@example.com\n");
    }
}
