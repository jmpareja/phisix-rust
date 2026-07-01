mod models;
mod parser;
mod scraper;
mod db;
mod routes;

use db::Db;
use routes::{archive_stocks, get_stocks_by_param, get_stocks_json, get_stocks_xml, AppState};
use axum::{
    routing::get,
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use tower_http::services::ServeDir;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "phisix_rust=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Get database path from environment or default to local phisix.db
    let db_path = std::env::var("DATABASE_PATH").unwrap_or_else(|_| "phisix.db".to_string());
    tracing::info!("Using database file: {}", db_path);
    let db = Db::open(&db_path)?;

    // Create state shared by routes
    let state = AppState {
        db,
        cache: Arc::new(tokio::sync::RwLock::new(None)),
        client: reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?,
    };

    // Configure routes
    let app = Router::new()
        // Live stocks endpoints
        .route("/stocks", get(get_stocks_json))
        .route("/stocks.json", get(get_stocks_json))
        .route("/stocks.xml", get(get_stocks_xml))
        // Suffix/date/parameter endpoint capturing everything under /stocks/
        .route("/stocks/{*param}", get(get_stocks_by_param))
        // Archive endpoints
        .route("/stocks/archive", get(archive_stocks).post(archive_stocks))
        .fallback_service(ServeDir::new("static"))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Run axum server
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("PHISIX API server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
