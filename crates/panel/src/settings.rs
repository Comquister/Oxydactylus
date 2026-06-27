use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::{AdminUser, AuthUser},
    db,
    error::{PanelError, Result},
    permissions::{SETTINGS_REINSTALL, SETTINGS_RENAME},
    servers::{check_server_access, fetch_server, get_node_client},
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct RenameServerRequest {
    name: String,
}

#[derive(Debug, Deserialize)]
pub struct ChangeEggRequest {
    egg_id: Uuid,
    image: String,
    #[serde(default)]
    reset_env: bool,
}

pub async fn rename_server(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RenameServerRequest>,
) -> Result<StatusCode> {
    if body.name.is_empty() {
        return Err(PanelError::Validation("name cannot be empty".to_string()));
    }

    let server = fetch_server(&state.db, id).await?;
    check_server_access(&user, &server, Some(SETTINGS_RENAME), &state.db).await?;

    let sql = db::port_sql(
        "UPDATE servers SET name = $1 WHERE id = $2",
        &state.db_backend,
    );
    sqlx::query(&sql)
        .bind(&body.name)
        .bind(server.id.to_string())
        .execute(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn reinstall_server(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let server = fetch_server(&state.db, id).await?;
    check_server_access(&user, &server, Some(SETTINGS_REINSTALL), &state.db).await?;

    let state_clone = state.clone();
    let server_clone = server.clone();
    tokio::spawn(async move {
        if let Ok(mut client) = get_node_client(&state_clone, server_clone.node_id).await {
            let ports = match fetch_server_ports(&state_clone, server_clone.id).await {
                Ok(p) => p,
                Err(_) => return,
            };
            let _ = client
                .provision(
                    &server_clone.id.to_string(),
                    &server_clone.image,
                    server_clone.memory_mb as u32,
                    server_clone.cpu_percent as u32,
                    server_clone.env.clone(),
                    ports,
                )
                .await;
        }
    });

    Ok(StatusCode::NO_CONTENT)
}

pub async fn change_egg(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ChangeEggRequest>,
) -> Result<StatusCode> {
    let server = fetch_server(&state.db, id).await?;

    let env = if body.reset_env {
        let mut new_env = Vec::new();
        if let Ok(egg_env) = crate::egg_vars::load_egg_env(&state.db, body.egg_id, Default::default()).await {
            new_env.extend(egg_env);
        }
        serde_json::to_string(&new_env).unwrap()
    } else {
        let mut env_map: std::collections::HashMap<String, String> = Default::default();
        for e in &server.env {
            if let Some((k, v)) = e.split_once('=') {
                env_map.insert(k.to_string(), v.to_string());
            }
        }
        if let Ok(egg_env) = crate::egg_vars::load_egg_env(&state.db, body.egg_id, Default::default()).await {
            for e in egg_env {
                if let Some((k, v)) = e.split_once('=') {
                    if !env_map.contains_key(k) {
                        env_map.insert(k.to_string(), v.to_string());
                    }
                }
            }
        }
        let env_vec: Vec<String> = env_map
            .into_iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        serde_json::to_string(&env_vec).unwrap()
    };

    let sql = db::port_sql(
        "UPDATE servers SET egg_id = $1, image = $2, env = $3 WHERE id = $4",
        &state.db_backend,
    );
    sqlx::query(&sql)
        .bind(body.egg_id.to_string())
        .bind(&body.image)
        .bind(&env)
        .bind(server.id.to_string())
        .execute(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn suspend_server(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let _server = fetch_server(&state.db, id).await?;

    let sql = db::port_sql(
        "UPDATE servers SET status = 'suspended' WHERE id = $1",
        &state.db_backend,
    );
    sqlx::query(&sql)
        .bind(id.to_string())
        .execute(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn unsuspend_server(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let _server = fetch_server(&state.db, id).await?;

    let sql = db::port_sql(
        "UPDATE servers SET status = 'stopped' WHERE id = $1 AND status = 'suspended'",
        &state.db_backend,
    );
    sqlx::query(&sql)
        .bind(id.to_string())
        .execute(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub fn settings_router() -> Router<AppState> {
    Router::new()
        .route("/reinstall", post(reinstall_server))
        .route("/change-egg", post(change_egg))
        .route("/suspend", post(suspend_server))
        .route("/unsuspend", post(unsuspend_server))
}

async fn fetch_server_ports(state: &AppState, server_id: Uuid) -> Result<Vec<String>> {
    let ports: Vec<String> = sqlx::query_scalar(
        "SELECT port FROM allocations WHERE server_id = $1 ORDER BY created_at",
    )
    .bind(server_id.to_string())
    .fetch_all(&state.db)
    .await?;
    Ok(ports.into_iter().map(|p: String| p).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        auth::{encode_token, hash_password},
        router, AppState,
    };
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use oxy_core::proto::node::{
        node_service_server::NodeService,
        ServerProvisionRequest, ServerReply, ServerStartRequest, ServerStopRequest,
        ServerDeleteRequest, ServerCommandRequest, ServerStats, ServerStatsRequest,
        ServerLogsRequest, LogLine, ListFilesRequest, ListFilesReply,
        GetFileContentsRequest, GetFileContentsReply, WriteFileContentsRequest,
        CreateDirectoryRequest, DeleteFilesRequest, RenameFileRequest,
        DownloadFileRequest, FileChunk,
    };
    use tokio_stream::wrappers::ReceiverStream;
    use tonic::{async_trait, Request as GrpcRequest, Response, Status};
    use tower::ServiceExt;

    const SECRET: &str = "test-secret-at-least-32-chars-long!!";

    async fn make_state(pool: sqlx::PgPool) -> AppState {
        use sqlx::ConnectOptions;
        sqlx::any::install_default_drivers();
        let db_url = pool.connect_options().to_url_lossy().to_string();
        let any_pool = sqlx::AnyPool::connect(&db_url).await.unwrap();
        AppState {
            db: any_pool,
            db_backend: "PostgreSQL".to_string(),
            jwt_secret: SECRET.to_string(),
            app_key: None,
        }
    }

    async fn seed_admin(pool: &sqlx::PgPool) -> (Uuid, String) {
        let id = Uuid::new_v4();
        let hash = hash_password("pass").unwrap();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, is_admin, created_at) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id.to_string())
        .bind("a@t.com")
        .bind(&hash)
        .bind(true)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .unwrap();

        (id, "pass".to_string())
    }

    async fn seed_user(pool: &sqlx::PgPool) -> (Uuid, String) {
        let id = Uuid::new_v4();
        let hash = hash_password("pass").unwrap();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, is_admin, created_at) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id.to_string())
        .bind("u@t.com")
        .bind(&hash)
        .bind(false)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .unwrap();

        (id, "pass".to_string())
    }

    async fn seed_node(pool: &sqlx::PgPool) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO nodes (id, name, grpc_addr, token, created_at) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id.to_string())
        .bind("test-node")
        .bind("127.0.0.1:50051")
        .bind("test-token")
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .unwrap();
        id
    }

    async fn seed_server(
        pool: &sqlx::PgPool,
        user_id: Uuid,
        node_id: Uuid,
    ) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, status, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(id.to_string())
        .bind(user_id.to_string())
        .bind(node_id.to_string())
        .bind("test-server")
        .bind("image:latest")
        .bind(512)
        .bind(50)
        .bind("stopped")
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .unwrap();
        id
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_rename_updates_name(pool: sqlx::PgPool) {
        let (admin_id, _) = seed_admin(&pool).await;
        let node_id = seed_node(&pool).await;
        let server_id = seed_server(&pool, admin_id, node_id).await;

        let state = make_state(pool).await;
        let token = encode_token(admin_id, true, "access", SECRET, 3600).unwrap();
        let router = router(state);

        let req = Request::builder()
            .method("PATCH")
            .uri(format!("/api/servers/{}", server_id))
            .header("Authorization", format!("Bearer {}", token))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"name": "new-name"}"#))
            .unwrap();

        let res = router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_suspend_prevents_start(pool: sqlx::PgPool) {
        let (admin_id, _) = seed_admin(&pool).await;
        let node_id = seed_node(&pool).await;
        let server_id = seed_server(&pool, admin_id, node_id).await;

        sqlx::query("UPDATE servers SET status = 'suspended' WHERE id = $1")
            .bind(server_id.to_string())
            .execute(&pool)
            .await
            .unwrap();

        let state = make_state(pool).await;
        let token = encode_token(admin_id, true, "access", SECRET, 3600).unwrap();
        let router = router(state);

        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/servers/{}/start", server_id))
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let res = router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_change_egg_requires_admin(pool: sqlx::PgPool) {
        let (admin_id, _) = seed_admin(&pool).await;
        let (user_id, _) = seed_user(&pool).await;
        let node_id = seed_node(&pool).await;
        let server_id = seed_server(&pool, admin_id, node_id).await;

        let state = make_state(pool).await;
        let token = encode_token(user_id, false, "access", SECRET, 3600).unwrap();
        let router = router(state);

        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/servers/{}/settings/change-egg", server_id))
            .header("Authorization", format!("Bearer {}", token))
            .header("content-type", "application/json")
            .body(Body::from(format!(r#"{{"egg_id": "{}", "image": "img"}}"#, Uuid::new_v4())))
            .unwrap();

        let res = router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }
}
