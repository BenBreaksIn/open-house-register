use open_house_register::{AppState, config::Config, db, router};
use tower::ServiceBuilder;
use vercel_runtime::{Error, axum::VercelLayer};
#[tokio::main]
async fn main() -> Result<(), Error> {
    let config = Config::from_env()?;
    let pool = db::connect(&config).await?;
    vercel_runtime::run(
        ServiceBuilder::new()
            .layer(VercelLayer::new())
            .service(router(AppState { pool, config })),
    )
    .await
}
