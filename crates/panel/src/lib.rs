pub mod allocations;
pub mod auth;
mod db;
pub mod egg_vars;
mod eggs;
pub mod error;
mod files;
pub mod node_client;
mod nodes;
pub mod permissions;
mod servers;
mod settings;
mod startup;
pub mod subusers;
mod users;

pub use error::{PanelError, Result};

use axum::{
    body::Body,
    http::{header, Uri},
    response::Response,
    routing::get,
};
use oxy_core::{OxyError, PanelConfig};
use rust_embed::RustEmbed;
use sqlx::AnyPool;

#[derive(RustEmbed)]
#[folder = "../frontend/dist/"]
struct FrontendAssets;

async fn frontend_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    match FrontendAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data))
                .unwrap()
        }
        None => {
            let index = FrontendAssets::get("index.html").unwrap();
            Response::builder()
                .header(header::CONTENT_TYPE, "text/html")
                .body(Body::from(index.data))
                .unwrap()
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::AnyPool,
    pub db_backend: String,
    pub jwt_secret: String,
    pub app_key: Option<String>,
}

pub fn router(state: AppState) -> axum::Router {
    axum::Router::new()
        .route("/api/me", get(users::me))
        .nest("/auth", auth::auth_router())
        .nest("/api/users", users::users_router())
        .nest("/api/nodes", nodes::nodes_router().merge(allocations::router()))
        .nest("/api/servers", servers::servers_router())
        .nest("/api/eggs", eggs::eggs_router())
        .with_state(state)
}

pub async fn run(config: PanelConfig) -> oxy_core::Result<()> {
    sqlx::any::install_default_drivers();

    let backend = if config.database_url.starts_with("mysql:") {
        "MySQL".to_string()
    } else if config.database_url.starts_with("sqlite:") {
        "SQLite".to_string()
    } else {
        "PostgreSQL".to_string()
    };

    let pool = db::create_pool(&config.database_url)
        .await
        .map_err(|e| OxyError::Config(e.to_string()))?;
    db::run_migrations(&pool)
        .await
        .map_err(|e| OxyError::Config(e.to_string()))?;
    let state = AppState {
        db: pool,
        db_backend: backend,
        jwt_secret: config.jwt_secret,
        app_key: config.app_key,
    };
    tracing::info!(listen = %config.http_listen, "panel starting");
    let listener = tokio::net::TcpListener::bind(&config.http_listen)
        .await
        .map_err(OxyError::Io)?;
    axum::serve(listener, router(state).fallback(frontend_handler))
        .await
        .map_err(OxyError::Io)
}
