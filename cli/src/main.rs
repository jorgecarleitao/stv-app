use sea_orm::Database;
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use server::*;

#[tokio::main]
async fn main() -> Result<(), String> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    tracing::info!("Logger initialized");

    // Initialize database
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:election.db".to_string());

    let db = Database::connect(&database_url)
        .await
        .map_err(|e| format!("Failed to connect to database: {}", e))?;

    // Run migrations
    use migration::MigratorTrait;
    migration::Migrator::up(&db, None)
        .await
        .map_err(|e| format!("Failed to run migrations: {}", e))?;

    let app = create_app(db).map_err(|e| {
        tracing::error!("Failed to create app: {}", e);
        e
    })?;

    let app = app.layer(
        TraceLayer::new_for_http()
            .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
            .on_request(DefaultOnRequest::new().level(Level::INFO))
            .on_response(DefaultOnResponse::new().level(Level::INFO)),
    );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    tracing::info!(addr = ?listener.local_addr().unwrap(), "listening");

    axum::serve(listener, app).await.map_err(|e| {
        tracing::error!("axum server error: {}", e);
        e.to_string()
    })
}
