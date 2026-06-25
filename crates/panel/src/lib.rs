mod db;
pub mod auth;
pub mod error;
pub mod node_client;
mod nodes;
mod servers;
mod users;
mod eggs;

pub use error::{PanelError, Result};

use oxy_core::{OxyError, PanelConfig};
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub db:         PgPool,
    pub jwt_secret: String,
}

pub fn router(state: AppState) -> axum::Router {
    axum::Router::new()
        .nest("/auth",        auth::auth_router())
        .nest("/api/users",   users::users_router())
        .nest("/api/nodes",   nodes::nodes_router())
        .nest("/api/servers", servers::servers_router())
        .nest("/api/eggs",    eggs::eggs_router())
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
        db:         pool,
        jwt_secret: config.jwt_secret,
    };
    tracing::info!(listen = %config.http_listen, "panel starting");
    let listener = tokio::net::TcpListener::bind(&config.http_listen)
        .await
        .map_err(OxyError::Io)?;
    axum::serve(listener, router(state))
        .await
        .map_err(OxyError::Io)
}
