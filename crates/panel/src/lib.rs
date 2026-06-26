pub mod auth;
mod db;
pub mod egg_vars;
mod eggs;
pub mod error;
pub mod node_client;
mod nodes;
pub mod permissions;
mod servers;
pub mod subusers;
mod users;

pub use error::{PanelError, Result};

use axum::routing::get;
use oxy_core::{OxyError, PanelConfig};
use sqlx::PgPool;
use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub jwt_secret: String,
}

pub fn router(state: AppState) -> axum::Router {
    axum::Router::new()
        .route("/api/me", get(users::me))
        .nest("/auth", auth::auth_router())
        .nest("/api/users", users::users_router())
        .nest("/api/nodes", nodes::nodes_router())
        .nest("/api/servers", servers::servers_router())
        .nest("/api/eggs", eggs::eggs_router())
        .with_state(state)
}

pub async fn run(config: PanelConfig) -> oxy_core::Result<()> {
    let pool = db::create_pool(&config.database_url)
        .await
        .map_err(|e| OxyError::Config(e.to_string()))?;
    db::run_migrations(&pool)
        .await
        .map_err(|e| OxyError::Config(e.to_string()))?;
    let state = AppState {
        db: pool,
        jwt_secret: config.jwt_secret,
    };
    tracing::info!(listen = %config.http_listen, "panel starting");
    let listener = tokio::net::TcpListener::bind(&config.http_listen)
        .await
        .map_err(OxyError::Io)?;
    let app = match config.public_dir {
        Some(dir) => {
            let spa = ServeDir::new(&dir)
                .not_found_service(ServeFile::new(format!("{}/index.html", dir)));
            router(state).fallback_service(spa)
        }
        None => router(state),
    };
    axum::serve(listener, app)
        .await
        .map_err(OxyError::Io)
}
