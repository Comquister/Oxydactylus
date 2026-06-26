use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use uuid::Uuid;

use crate::{
    auth::{AdminUser, AuthUser},
    error::{PanelError, Result},
    node_client::NodeClient,
    permissions::{CONTROL_CONSOLE, CONTROL_RESTART, CONTROL_START, CONTROL_STOP},
    subusers, AppState,
};

#[derive(Debug, Serialize)]
pub struct Server {
    pub id: Uuid,
    pub user_id: Uuid,
    pub node_id: Uuid,
    pub allocation_id: Option<Uuid>,
    pub name: String,
    pub image: String,
    pub memory_mb: i32,
    pub cpu_percent: i32,
    pub env: Vec<String>,
    pub status: String,
    pub database_limit: i32,
    pub backup_limit: i32,
    pub allocation_limit: i32,
    pub created_at: DateTime<Utc>,
}

impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for Server {
    fn from_row(row: &'r sqlx::any::AnyRow) -> std::result::Result<Self, sqlx::Error> {
        use sqlx::Row;
        let id_str: String = row.try_get("id")?;
        let id = Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let user_id_str: String = row.try_get("user_id")?;
        let user_id = Uuid::parse_str(&user_id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let node_id_str: String = row.try_get("node_id")?;
        let node_id = Uuid::parse_str(&node_id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let allocation_id_opt: Option<String> = row.try_get("allocation_id")?;
        let allocation_id = match allocation_id_opt {
            Some(s) if !s.is_empty() => Some(Uuid::parse_str(&s).map_err(|e| sqlx::Error::Decode(Box::new(e)))?),
            _ => None,
        };

        let env_str: String = row.try_get("env")?;
        let env: Vec<String> = serde_json::from_str(&env_str)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let created_at_str: String = row.try_get("created_at")?;
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        Ok(Self {
            id,
            user_id,
            node_id,
            allocation_id,
            name: row.try_get("name")?,
            image: row.try_get("image")?,
            memory_mb: row.try_get("memory_mb")?,
            cpu_percent: row.try_get("cpu_percent")?,
            env,
            status: row.try_get("status")?,
            database_limit: row.try_get("database_limit")?,
            backup_limit: row.try_get("backup_limit")?,
            allocation_limit: row.try_get("allocation_limit")?,
            created_at,
        })
    }
}

#[derive(Debug)]
struct NodeRow {
    grpc_addr: String,
    token: String,
}

impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for NodeRow {
    fn from_row(row: &'r sqlx::any::AnyRow) -> std::result::Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(Self {
            grpc_addr: row.try_get("grpc_addr")?,
            token: row.try_get("token")?,
        })
    }
}

async fn get_node_client(state: &AppState, node_id: Uuid) -> Result<NodeClient> {
    let row = sqlx::query_as::<_, NodeRow>("SELECT grpc_addr, token FROM nodes WHERE id = $1")
        .bind(node_id.to_string())
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| PanelError::NotFound(format!("node {}", node_id)))?;
    NodeClient::connect(&row.grpc_addr, &row.token).await
}

async fn fetch_server(db: &sqlx::AnyPool, id: Uuid) -> Result<Server> {
    sqlx::query_as::<_, Server>(
        "SELECT id, user_id, node_id, allocation_id, name, image, memory_mb, cpu_percent, env, status, database_limit, backup_limit, allocation_limit, created_at
         FROM servers WHERE id = $1",
    )
    .bind(id.to_string())
    .fetch_one(db)
    .await
    .map_err(Into::into)
}

pub(crate) async fn check_server_access(
    user: &AuthUser,
    server: &Server,
    perm: Option<&str>,
    db: &sqlx::AnyPool,
) -> Result<()> {
    if user.is_admin || server.user_id == user.id {
        return Ok(());
    }
    let permissions_str: Option<String> = sqlx::query_scalar(
        "SELECT permissions FROM server_subusers
         WHERE server_id = $1 AND user_id = $2",
    )
    .bind(server.id.to_string())
    .bind(user.id.to_string())
    .fetch_optional(db)
    .await?;

    let perms: Vec<String> = match permissions_str {
        Some(s) => serde_json::from_str(&s).map_err(|e| PanelError::Internal(e.to_string()))?,
        None => return Err(PanelError::Forbidden),
    };

    if perms.is_empty() {
        return Err(PanelError::Forbidden);
    }
    if let Some(p) = perm {
        if !perms.iter().any(|s| s == p) {
            return Err(PanelError::Forbidden);
        }
    }
    Ok(())
}

async fn list_servers(State(state): State<AppState>, user: AuthUser) -> Result<Json<Vec<Server>>> {
    let servers = if user.is_admin {
        sqlx::query_as::<_, Server>(
            "SELECT id, user_id, node_id, allocation_id, name, image, memory_mb, cpu_percent, env, status, database_limit, backup_limit, allocation_limit, created_at
             FROM servers ORDER BY created_at",
        )
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as::<_, Server>(
            "SELECT id, user_id, node_id, allocation_id, name, image, memory_mb, cpu_percent, env, status, database_limit, backup_limit, allocation_limit, created_at
             FROM servers WHERE user_id = $1 ORDER BY created_at",
        )
        .bind(user.id.to_string())
        .fetch_all(&state.db)
        .await?
    };
    Ok(Json(servers))
}

#[derive(Debug, Deserialize)]
struct CreateServerRequest {
    node_id: Uuid,
    name: String,
    image: String,
    memory_mb: i32,
    cpu_percent: i32,
    #[serde(default)]
    env: Vec<String>,
    #[serde(default)]
    egg_id: Option<Uuid>,
    #[serde(default)]
    egg_vars: std::collections::HashMap<String, String>,
    #[serde(default)]
    user_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct LogsQuery {
    #[serde(default)]
    follow: bool,
}

async fn create_server(
    State(state): State<AppState>,
    admin: AdminUser,
    Json(body): Json<CreateServerRequest>,
) -> Result<(StatusCode, Json<Server>)> {
    if body.memory_mb <= 0 || body.cpu_percent <= 0 {
        return Err(PanelError::Validation(
            "memory_mb and cpu_percent must be positive".to_string(),
        ));
    }
    if body.name.is_empty() || body.image.is_empty() {
        return Err(PanelError::Validation(
            "name and image are required".to_string(),
        ));
    }

    let owner_id = body.user_id.unwrap_or(admin.0.id);

    // resolve egg variables if an egg_id was given
    let mut env = body.env.clone();
    if let Some(eid) = body.egg_id {
        let egg_env = crate::egg_vars::load_egg_env(&state.db, eid, body.egg_vars.clone()).await?;
        env.extend(egg_env);
    }

    let id = Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();
    let sql = crate::db::port_sql(
        "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, env, egg_id, status, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'installing', $10)
         RETURNING id, user_id, node_id, allocation_id, name, image, memory_mb, cpu_percent, env, status, database_limit, backup_limit, allocation_limit, created_at",
        &state.db_backend,
    );
    let mut server = sqlx::query_as::<_, Server>(&sql)
        .bind(&id)
        .bind(owner_id.to_string())
        .bind(body.node_id.to_string())
        .bind(&body.name)
        .bind(&body.image)
        .bind(body.memory_mb)
        .bind(body.cpu_percent)
        .bind(serde_json::to_string(&env).unwrap())
        .bind(body.egg_id.map(|id| id.to_string()))
        .bind(&created_at)
        .fetch_one(&state.db)
    .await?;

    let mut client = get_node_client(&state, server.node_id).await?;
    if let Err(e) = client
        .provision(
            &server.id.to_string(),
            &server.image,
            server.memory_mb as u32,
            server.cpu_percent as u32,
            env,
        )
        .await
    {
        // compensating delete — best effort; ignore failure
        let _ = sqlx::query("DELETE FROM servers WHERE id = $1")
            .bind(server.id.to_string())
            .execute(&state.db)
            .await;
        return Err(e);
    }

    sqlx::query("UPDATE servers SET status = 'stopped' WHERE id = $1")
        .bind(server.id.to_string())
        .execute(&state.db)
        .await?;
    server.status = "stopped".to_string();

    Ok((StatusCode::CREATED, Json(server)))
}

async fn get_server(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Server>> {
    let server = fetch_server(&state.db, id).await?;
    if !user.is_admin && server.user_id != user.id {
        return Err(PanelError::Forbidden);
    }
    Ok(Json(server))
}

async fn delete_server(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let server = fetch_server(&state.db, id).await?;

    if let Ok(mut client) = get_node_client(&state, server.node_id).await {
        let _ = client.stop(&server.id.to_string(), 10).await;
        let _ = client.delete(&server.id.to_string()).await;
    }

    sqlx::query("DELETE FROM servers WHERE id = $1")
        .bind(id.to_string())
        .execute(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn start_server(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let server = fetch_server(&state.db, id).await?;
    check_server_access(&user, &server, Some(CONTROL_START), &state.db).await?;
    let mut client = get_node_client(&state, server.node_id).await?;
    match client.start(&server.id.to_string()).await {
        Ok(_) => {
            sqlx::query("UPDATE servers SET status = 'running' WHERE id = $1")
                .bind(server.id.to_string())
                .execute(&state.db)
                .await?;
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            let _ = sqlx::query("UPDATE servers SET status = 'error' WHERE id = $1")
                .bind(server.id.to_string())
                .execute(&state.db)
                .await;
            Err(e)
        }
    }
}

async fn stop_server(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let server = fetch_server(&state.db, id).await?;
    check_server_access(&user, &server, Some(CONTROL_STOP), &state.db).await?;
    let mut client = get_node_client(&state, server.node_id).await?;
    client.stop(&server.id.to_string(), 10).await?;
    sqlx::query("UPDATE servers SET status = 'stopped' WHERE id = $1")
        .bind(server.id.to_string())
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn restart_server(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let server = fetch_server(&state.db, id).await?;
    check_server_access(&user, &server, Some(CONTROL_RESTART), &state.db).await?;
    let mut client = get_node_client(&state, server.node_id).await?;
    // stop best-effort — ignora se já estiver parado
    let _ = client.stop(&server.id.to_string(), 10).await;
    match client.start(&server.id.to_string()).await {
        Ok(_) => {
            sqlx::query("UPDATE servers SET status = 'running' WHERE id = $1")
                .bind(server.id.to_string())
                .execute(&state.db)
                .await?;
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            let _ = sqlx::query("UPDATE servers SET status = 'error' WHERE id = $1")
                .bind(server.id.to_string())
                .execute(&state.db)
                .await;
            Err(e)
        }
    }
}

async fn provision_server(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let server = fetch_server(&state.db, id).await?;

    let mut client = get_node_client(&state, server.node_id).await?;
    client
        .provision(
            &server.id.to_string(),
            &server.image,
            server.memory_mb as u32,
            server.cpu_percent as u32,
            server.env.clone(),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn server_command(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<StatusCode> {
    let content = body["content"]
        .as_str()
        .ok_or_else(|| PanelError::Validation("content field required".to_string()))?
        .to_string();
    let server = fetch_server(&state.db, id).await?;
    check_server_access(&user, &server, Some(CONTROL_CONSOLE), &state.db).await?;
    let mut client = get_node_client(&state, server.node_id).await?;
    client
        .send_command(&server.id.to_string(), &content)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn server_stats(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    let server = fetch_server(&state.db, id).await?;
    check_server_access(&user, &server, None, &state.db).await?;
    let mut client = get_node_client(&state, server.node_id).await?;
    let stats = client.get_stats(&server.id.to_string()).await?;
    Ok(Json(serde_json::json!({
        "memory_bytes": stats.memory_bytes,
        "cpu_percent":  stats.cpu_percent,
        "rx_bytes":     stats.rx_bytes,
        "tx_bytes":     stats.tx_bytes,
    })))
}

async fn stream_server_logs(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<LogsQuery>,
) -> Result<Sse<impl futures_util::Stream<Item = std::result::Result<Event, Infallible>> + Send>> {
    let server = fetch_server(&state.db, id).await?;
    check_server_access(&user, &server, Some(CONTROL_CONSOLE), &state.db).await?;
    let mut client = get_node_client(&state, server.node_id).await?;
    let log_stream = client.stream_logs(&server.id.to_string(), q.follow).await?;
    let sse_stream = log_stream.map(|result| {
        let event = match result {
            Ok(line) => Event::default()
                .event(line.stream)
                .data(line.content.trim_end_matches(['\r', '\n'])),
            Err(e) => Event::default().event("error").data(e.to_string()),
        };
        Ok::<Event, Infallible>(event)
    });
    Ok(Sse::new(sse_stream))
}

pub fn servers_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_servers).post(create_server))
        .route("/:id", get(get_server).delete(delete_server))
        .route("/:id/start", post(start_server))
        .route("/:id/stop", post(stop_server))
        .route("/:id/restart", post(restart_server))
        .route("/:id/provision", post(provision_server))
        .route("/:id/command", post(server_command))
        .route("/:id/stats", get(server_stats))
        .route("/:id/logs", get(stream_server_logs))
        .route(
            "/:id/subusers",
            get(subusers::list_subusers).post(subusers::create_subuser),
        )
        .route(
            "/:id/subusers/:uid",
            axum::routing::patch(subusers::update_subuser).delete(subusers::delete_subuser),
        )
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
        node_service_server::{NodeService, NodeServiceServer},
        LogLine, ServerCommandRequest, ServerDeleteRequest, ServerLogsRequest,
        ServerProvisionRequest, ServerReply, ServerStartRequest, ServerStats, ServerStatsRequest,
        ServerStopRequest,
    };
    use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
    use tonic::{async_trait, Request as GrpcRequest, Response, Status};
    use tower::ServiceExt;
    use uuid::Uuid;

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
        let token = encode_token(id, true, "access", SECRET, 900).unwrap();
        (id, token)
    }

    struct AcceptAllNode;

    #[async_trait]
    impl NodeService for AcceptAllNode {
        type StreamLogsStream = ReceiverStream<std::result::Result<LogLine, Status>>;
        async fn provision_server(
            &self,
            _: GrpcRequest<ServerProvisionRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn start_server(
            &self,
            _: GrpcRequest<ServerStartRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn stop_server(
            &self,
            _: GrpcRequest<ServerStopRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn delete_server(
            &self,
            _: GrpcRequest<ServerDeleteRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn send_command(
            &self,
            _: GrpcRequest<ServerCommandRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn get_stats(
            &self,
            req: GrpcRequest<ServerStatsRequest>,
        ) -> std::result::Result<Response<ServerStats>, Status> {
            Ok(Response::new(ServerStats {
                server_id: req.into_inner().server_id,
                memory_bytes: 1024,
                cpu_percent: 10.0,
                rx_bytes: 50,
                tx_bytes: 100,
            }))
        }
        async fn stream_logs(
            &self,
            _: GrpcRequest<ServerLogsRequest>,
        ) -> std::result::Result<Response<Self::StreamLogsStream>, Status> {
            let (_, rx) = tokio::sync::mpsc::channel(1);
            Ok(Response::new(ReceiverStream::new(rx)))
        }
    }

    async fn start_mock_node(token: &str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let t = token.to_string();
        tokio::spawn(async move {
            use oxy_node::interceptor::AuthInterceptor;
            tonic::transport::Server::builder()
                .add_service(NodeServiceServer::with_interceptor(
                    AcceptAllNode,
                    AuthInterceptor::new(&t),
                ))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        });
        format!("http://127.0.0.1:{}", port)
    }

    async fn seed_node(pool: &sqlx::PgPool, grpc_addr: &str) -> Uuid {
        let id_str: String = sqlx::query_scalar(
            "INSERT INTO nodes (id, name, grpc_addr, token, created_at) VALUES ($1, $2, $3, $4, $5) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind("test-node")
        .bind(grpc_addr)
        .bind("node-token")
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(pool)
        .await
        .unwrap();
        Uuid::parse_str(&id_str).unwrap()
    }

    struct LogNode;

    #[async_trait]
    impl NodeService for LogNode {
        type StreamLogsStream = ReceiverStream<std::result::Result<LogLine, Status>>;

        async fn provision_server(
            &self,
            _: GrpcRequest<ServerProvisionRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn start_server(
            &self,
            _: GrpcRequest<ServerStartRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn stop_server(
            &self,
            _: GrpcRequest<ServerStopRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn delete_server(
            &self,
            _: GrpcRequest<ServerDeleteRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn send_command(
            &self,
            _: GrpcRequest<ServerCommandRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn get_stats(
            &self,
            req: GrpcRequest<ServerStatsRequest>,
        ) -> std::result::Result<Response<ServerStats>, Status> {
            Ok(Response::new(ServerStats {
                server_id: req.into_inner().server_id,
                memory_bytes: 0,
                cpu_percent: 0.0,
                rx_bytes: 0,
                tx_bytes: 0,
            }))
        }
        async fn stream_logs(
            &self,
            _: GrpcRequest<ServerLogsRequest>,
        ) -> std::result::Result<Response<Self::StreamLogsStream>, Status> {
            let (tx, rx) = tokio::sync::mpsc::channel(4);
            tokio::spawn(async move {
                let _ = tx
                    .send(Ok(LogLine {
                        content: "starting up\n".into(),
                        stream: "stdout".into(),
                        timestamp: 0,
                    }))
                    .await;
                let _ = tx
                    .send(Ok(LogLine {
                        content: "ready\n".into(),
                        stream: "stdout".into(),
                        timestamp: 0,
                    }))
                    .await;
            });
            Ok(Response::new(ReceiverStream::new(rx)))
        }
    }

    async fn start_log_node(token: &str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let t = token.to_string();
        tokio::spawn(async move {
            use oxy_node::interceptor::AuthInterceptor;
            tonic::transport::Server::builder()
                .add_service(NodeServiceServer::with_interceptor(
                    LogNode,
                    AuthInterceptor::new(&t),
                ))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        });
        format!("http://127.0.0.1:{}", port)
    }

    struct FailStartNode;

    #[async_trait]
    impl NodeService for FailStartNode {
        type StreamLogsStream = ReceiverStream<std::result::Result<LogLine, Status>>;
        async fn provision_server(
            &self,
            _: GrpcRequest<ServerProvisionRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn start_server(
            &self,
            _: GrpcRequest<ServerStartRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Err(Status::internal("docker error"))
        }
        async fn stop_server(
            &self,
            _: GrpcRequest<ServerStopRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn delete_server(
            &self,
            _: GrpcRequest<ServerDeleteRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn send_command(
            &self,
            _: GrpcRequest<ServerCommandRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn get_stats(
            &self,
            _: GrpcRequest<ServerStatsRequest>,
        ) -> std::result::Result<Response<ServerStats>, Status> {
            Err(Status::internal("not implemented"))
        }
        async fn stream_logs(
            &self,
            _: GrpcRequest<ServerLogsRequest>,
        ) -> std::result::Result<Response<Self::StreamLogsStream>, Status> {
            let (_, rx) = tokio::sync::mpsc::channel(1);
            Ok(Response::new(ReceiverStream::new(rx)))
        }
    }

    async fn start_fail_node(token: &str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let t = token.to_string();
        tokio::spawn(async move {
            use oxy_node::interceptor::AuthInterceptor;
            tonic::transport::Server::builder()
                .add_service(NodeServiceServer::with_interceptor(
                    FailStartNode,
                    AuthInterceptor::new(&t),
                ))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        });
        format!("http://127.0.0.1:{}", port)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn stream_logs_returns_sse_events(pool: sqlx::PgPool) {
        let node_addr = start_log_node("node-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (admin_id, token) = seed_admin(&pool).await;
        let node_id = seed_node(&pool, &node_addr).await;

        let server_id_str: String = sqlx::query_scalar(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(admin_id.to_string())
        .bind(node_id.to_string())
        .bind("log-srv")
        .bind("ubuntu")
        .bind(512)
        .bind(50)
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let server_id = Uuid::parse_str(&server_id_str).unwrap();

        let app = router(make_state(pool).await);
        let req = Request::builder()
            .method("GET")
            .uri(format!("/api/servers/{}/logs", server_id))
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();

        assert_eq!(res.status(), StatusCode::OK);
        let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("text/event-stream"), "expected SSE, got {}", ct);

        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(
            body.contains("data:"),
            "SSE body missing data lines:\n{}",
            body
        );
        assert!(
            body.contains("starting up"),
            "first log line missing:\n{}",
            body
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_server_provisions_on_node(pool: sqlx::PgPool) {
        let node_addr = start_mock_node("node-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (_, token) = seed_admin(&pool).await;
        let node_id = seed_node(&pool, &node_addr).await;

        let app = router(make_state(pool).await);
        let body = serde_json::json!({
            "node_id":     node_id,
            "name":        "mc-server-1",
            "image":       "itzg/minecraft-server",
            "memory_mb":   1024,
            "cpu_percent": 100,
            "env":         ["EULA=TRUE"]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/servers")
            .header("authorization", format!("Bearer {}", token))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_server_assigns_user_id(pool: sqlx::PgPool) {
        let node_addr = start_mock_node("node-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (admin_id, token) = seed_admin(&pool).await;
        let node_id = seed_node(&pool, &node_addr).await;

        // criar segundo usuário para testar atribuição explícita
        let owner_id_str: String = sqlx::query_scalar(
            "INSERT INTO users (id, email, password_hash, created_at) VALUES ($1, $2, $3, $4) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind("owner@test.com")
        .bind("$argon2id$v=19$m=19456,t=2,p=1$fakehash")
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let owner_id = Uuid::parse_str(&owner_id_str).unwrap();

        let app = router(make_state(pool).await);
        let body = serde_json::json!({
            "node_id":     node_id,
            "user_id":     owner_id,
            "name":        "owned-server",
            "image":       "ubuntu",
            "memory_mb":   512,
            "cpu_percent": 50,
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/servers")
            .header("authorization", format!("Bearer {}", token))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let srv: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(srv["user_id"].as_str().unwrap(), owner_id.to_string());
        // suppress unused variable warning
        let _ = admin_id;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_server_defaults_user_id_to_admin(pool: sqlx::PgPool) {
        let node_addr = start_mock_node("node-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (admin_id, token) = seed_admin(&pool).await;
        let node_id = seed_node(&pool, &node_addr).await;

        let app = router(make_state(pool).await);
        let body = serde_json::json!({
            "node_id":     node_id,
            "name":        "admin-server",
            "image":       "ubuntu",
            "memory_mb":   512,
            "cpu_percent": 50,
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/servers")
            .header("authorization", format!("Bearer {}", token))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let srv: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(srv["user_id"].as_str().unwrap(), admin_id.to_string());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_server_returns_stopped_status(pool: sqlx::PgPool) {
        let node_addr = start_mock_node("node-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (_, token) = seed_admin(&pool).await;
        let node_id = seed_node(&pool, &node_addr).await;

        let app = router(make_state(pool).await);
        let body = serde_json::json!({
            "node_id":     node_id,
            "name":        "status-test",
            "image":       "ubuntu",
            "memory_mb":   512,
            "cpu_percent": 50,
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/servers")
            .header("authorization", format!("Bearer {}", token))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let srv: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(srv["status"], "stopped");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn start_server_sets_running_status(pool: sqlx::PgPool) {
        let node_addr = start_mock_node("node-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (admin_id, token) = seed_admin(&pool).await;
        let node_id = seed_node(&pool, &node_addr).await;
        let server_id_str: String = sqlx::query_scalar(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(admin_id.to_string())
        .bind(node_id.to_string())
        .bind("start-srv")
        .bind("ubuntu")
        .bind(512)
        .bind(50)
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let server_id = Uuid::parse_str(&server_id_str).unwrap();

        let app = router(make_state(pool.clone()).await);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/servers/{}/start", server_id))
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);

        let status: String = sqlx::query_scalar("SELECT status FROM servers WHERE id = $1")
            .bind(server_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "running");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn start_server_sets_error_on_node_failure(pool: sqlx::PgPool) {
        let node_addr = start_fail_node("node-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (admin_id, token) = seed_admin(&pool).await;
        let node_id = seed_node(&pool, &node_addr).await;
        let server_id_str: String = sqlx::query_scalar(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(admin_id.to_string())
        .bind(node_id.to_string())
        .bind("fail-srv")
        .bind("ubuntu")
        .bind(512)
        .bind(50)
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let server_id = Uuid::parse_str(&server_id_str).unwrap();

        let app = router(make_state(pool.clone()).await);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/servers/{}/start", server_id))
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert!(
            res.status().is_server_error(),
            "expected 5xx, got {}",
            res.status()
        );

        let status: String = sqlx::query_scalar("SELECT status FROM servers WHERE id = $1")
            .bind(server_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "error");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn stop_server_sets_stopped_status(pool: sqlx::PgPool) {
        let node_addr = start_mock_node("node-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (admin_id, token) = seed_admin(&pool).await;
        let node_id = seed_node(&pool, &node_addr).await;
        let server_id_str: String = sqlx::query_scalar(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(admin_id.to_string())
        .bind(node_id.to_string())
        .bind("stop-srv")
        .bind("ubuntu")
        .bind(512)
        .bind(50)
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let server_id = Uuid::parse_str(&server_id_str).unwrap();

        sqlx::query("UPDATE servers SET status = 'running' WHERE id = $1")
            .bind(server_id.to_string())
            .execute(&pool)
            .await
            .unwrap();

        let app = router(make_state(pool.clone()).await);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/servers/{}/stop", server_id))
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);

        let status: String = sqlx::query_scalar("SELECT status FROM servers WHERE id = $1")
            .bind(server_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "stopped");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn list_servers_returns_empty(pool: sqlx::PgPool) {
        let (_, token) = seed_admin(&pool).await;
        let app = router(make_state(pool).await);
        let req = Request::builder()
            .method("GET")
            .uri("/api/servers")
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json.as_array().unwrap().len(), 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_stats_proxies_to_node(pool: sqlx::PgPool) {
        let node_addr = start_mock_node("node-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (admin_id, token) = seed_admin(&pool).await;
        let node_id = seed_node(&pool, &node_addr).await;
        let server_id_str: String = sqlx::query_scalar(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(admin_id.to_string())
        .bind(node_id.to_string())
        .bind("srv-x")
        .bind("ubuntu")
        .bind(512)
        .bind(50)
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let server_id = Uuid::parse_str(&server_id_str).unwrap();

        let app = router(make_state(pool).await);
        let req = Request::builder()
            .method("GET")
            .uri(format!("/api/servers/{}/stats", server_id))
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["memory_bytes"], 1024);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_server_with_egg_resolves_vars(pool: sqlx::PgPool) {
        let node_addr = start_mock_node("node-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (_, token) = seed_admin(&pool).await;
        let node_id = seed_node(&pool, &node_addr).await;

        // create egg + variable
        let egg_id_str: String = sqlx::query_scalar(
            "INSERT INTO eggs (id, name, start_cmd, docker_images, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind("Purpur")
        .bind("java -jar server.jar")
        .bind(serde_json::to_string(&serde_json::json!({})).unwrap())
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let egg_id = Uuid::parse_str(&egg_id_str).unwrap();

        sqlx::query(
            "INSERT INTO egg_variables (id, egg_id, name, env_variable, default_val, field_type)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(egg_id.to_string())
        .bind("Jar")
        .bind("SERVER_JARFILE")
        .bind("server.jar")
        .bind("text")
        .execute(&pool)
        .await
        .unwrap();

        let app = router(make_state(pool).await);
        let body = serde_json::json!({
            "node_id":     node_id,
            "name":        "mc-egg-server",
            "image":       "itzg/minecraft-server",
            "memory_mb":   1024,
            "cpu_percent": 100,
            "egg_id":      egg_id,
            "egg_vars":    {"SERVER_JARFILE": "purpur.jar"}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/servers")
            .header("authorization", format!("Bearer {}", token))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let srv: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        // env should contain the resolved var
        let env_arr = srv["env"].as_array().unwrap();
        assert!(env_arr
            .iter()
            .any(|v| v.as_str() == Some("SERVER_JARFILE=purpur.jar")));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn list_servers_filters_by_owner(pool: sqlx::PgPool) {
        let node_addr = start_mock_node("node-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (admin_id, admin_token) = seed_admin(&pool).await;
        let node_id = seed_node(&pool, &node_addr).await;

        // criar segundo usuário
        let other_id_str: String = sqlx::query_scalar(
            "INSERT INTO users (id, email, password_hash, created_at) VALUES ($1, $2, $3, $4) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind("other@test.com")
        .bind("$argon2id$v=19$m=19456,t=2,p=1$fakehash")
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let other_id = Uuid::parse_str(&other_id_str).unwrap();
        let other_token =
            crate::auth::encode_token(other_id, false, "access", SECRET, 900).unwrap();

        // admin cria servidor para si mesmo
        sqlx::query(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(admin_id.to_string())
        .bind(node_id.to_string())
        .bind("admin-srv")
        .bind("ubuntu")
        .bind(512)
        .bind(50)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();

        // admin cria servidor para other
        sqlx::query(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(other_id.to_string())
        .bind(node_id.to_string())
        .bind("other-srv")
        .bind("ubuntu")
        .bind(512)
        .bind(50)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();

        let app = router(make_state(pool).await);

        // admin vê os dois
        let req = Request::builder()
            .method("GET")
            .uri("/api/servers")
            .header("authorization", format!("Bearer {}", admin_token))
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let list: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(list.as_array().unwrap().len(), 2);

        // other vê só o seu
        let req = Request::builder()
            .method("GET")
            .uri("/api/servers")
            .header("authorization", format!("Bearer {}", other_token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let list: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(list[0]["name"], "other-srv");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_server_forbidden_for_non_owner(pool: sqlx::PgPool) {
        let (admin_id, _) = seed_admin(&pool).await;
        let node_addr = start_mock_node("node-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let node_id = seed_node(&pool, &node_addr).await;

        let other_id_str: String = sqlx::query_scalar(
            "INSERT INTO users (id, email, password_hash, created_at) VALUES ($1, $2, $3, $4) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind("other2@test.com")
        .bind("$argon2id$v=19$m=19456,t=2,p=1$fakehash")
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let other_id = Uuid::parse_str(&other_id_str).unwrap();
        let other_token =
            crate::auth::encode_token(other_id, false, "access", SECRET, 900).unwrap();

        let server_id_str: String = sqlx::query_scalar(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(admin_id.to_string())
        .bind(node_id.to_string())
        .bind("forbidden-srv")
        .bind("ubuntu")
        .bind(512)
        .bind(50)
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let server_id = Uuid::parse_str(&server_id_str).unwrap();

        let app = router(make_state(pool).await);
        let req = Request::builder()
            .method("GET")
            .uri(format!("/api/servers/{}", server_id))
            .header("authorization", format!("Bearer {}", other_token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn subuser_with_start_can_start_server(pool: sqlx::PgPool) {
        let node_addr = start_mock_node("node-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (admin_id, _) = seed_admin(&pool).await;
        let node_id = seed_node(&pool, &node_addr).await;

        let subuser_id_str: String = sqlx::query_scalar(
            "INSERT INTO users (id, email, password_hash, created_at) VALUES ($1, $2, $3, $4) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind("sub@test.com")
        .bind("$argon2id$v=19$m=19456,t=2,p=1$fakehash")
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let subuser_id = Uuid::parse_str(&subuser_id_str).unwrap();
        let sub_token =
            crate::auth::encode_token(subuser_id, false, "access", SECRET, 900).unwrap();

        let server_id_str: String = sqlx::query_scalar(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(admin_id.to_string())
        .bind(node_id.to_string())
        .bind("sub-start-srv")
        .bind("ubuntu")
        .bind(512)
        .bind(50)
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let server_id = Uuid::parse_str(&server_id_str).unwrap();

        // adicionar subuser com control.start
        sqlx::query(
            "INSERT INTO server_subusers (id, server_id, user_id, permissions, created_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(server_id.to_string())
        .bind(subuser_id.to_string())
        .bind(serde_json::to_string(&vec!["control.start"]).unwrap())
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();

        let app = router(make_state(pool).await);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/servers/{}/start", server_id))
            .header("authorization", format!("Bearer {}", sub_token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn subuser_without_stop_gets_403(pool: sqlx::PgPool) {
        let (admin_id, _) = seed_admin(&pool).await;
        let node_addr = start_mock_node("node-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let node_id = seed_node(&pool, &node_addr).await;

        let subuser_id_str: String = sqlx::query_scalar(
            "INSERT INTO users (id, email, password_hash, created_at) VALUES ($1, $2, $3, $4) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind("nosub@test.com")
        .bind("$argon2id$v=19$m=19456,t=2,p=1$fakehash")
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let subuser_id = Uuid::parse_str(&subuser_id_str).unwrap();
        let sub_token =
            crate::auth::encode_token(subuser_id, false, "access", SECRET, 900).unwrap();

        let server_id_str: String = sqlx::query_scalar(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(admin_id.to_string())
        .bind(node_id.to_string())
        .bind("nosub-srv")
        .bind("ubuntu")
        .bind(512)
        .bind(50)
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let server_id = Uuid::parse_str(&server_id_str).unwrap();

        // subuser com control.start mas NÃO control.stop
        sqlx::query(
            "INSERT INTO server_subusers (id, server_id, user_id, permissions, created_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(server_id.to_string())
        .bind(subuser_id.to_string())
        .bind(serde_json::to_string(&vec!["control.start"]).unwrap())
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();

        let app = router(make_state(pool).await);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/servers/{}/stop", server_id))
            .header("authorization", format!("Bearer {}", sub_token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn stranger_gets_403_on_start(pool: sqlx::PgPool) {
        let (admin_id, _) = seed_admin(&pool).await;
        let node_addr = start_mock_node("node-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let node_id = seed_node(&pool, &node_addr).await;

        let stranger_id_str: String = sqlx::query_scalar(
            "INSERT INTO users (id, email, password_hash, created_at) VALUES ($1, $2, $3, $4) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind("stranger@test.com")
        .bind("$argon2id$v=19$m=19456,t=2,p=1$fakehash")
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let stranger_id = Uuid::parse_str(&stranger_id_str).unwrap();
        let stranger_token =
            crate::auth::encode_token(stranger_id, false, "access", SECRET, 900).unwrap();

        let server_id_str: String = sqlx::query_scalar(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(admin_id.to_string())
        .bind(node_id.to_string())
        .bind("stranger-srv")
        .bind("ubuntu")
        .bind(512)
        .bind(50)
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let server_id = Uuid::parse_str(&server_id_str).unwrap();

        let app = router(make_state(pool).await);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/servers/{}/start", server_id))
            .header("authorization", format!("Bearer {}", stranger_token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn restart_server_transitions_to_running(pool: sqlx::PgPool) {
        let node_addr = start_mock_node("node-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (admin_id, token) = seed_admin(&pool).await;
        let node_id = seed_node(&pool, &node_addr).await;

        let server_id_str: String = sqlx::query_scalar(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(admin_id.to_string())
        .bind(node_id.to_string())
        .bind("restart-srv")
        .bind("ubuntu")
        .bind(512)
        .bind(50)
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let server_id = Uuid::parse_str(&server_id_str).unwrap();

        let app = router(make_state(pool.clone()).await);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/servers/{}/restart", server_id))
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);

        let status: String = sqlx::query_scalar("SELECT status FROM servers WHERE id = $1")
            .bind(server_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "running");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn restart_forbidden_for_non_owner(pool: sqlx::PgPool) {
        let (admin_id, _) = seed_admin(&pool).await;
        let node_addr = start_mock_node("node-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let node_id = seed_node(&pool, &node_addr).await;

        let other_id_str: String = sqlx::query_scalar(
            "INSERT INTO users (id, email, password_hash, created_at) VALUES ($1, $2, $3, $4) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind("rother@test.com")
        .bind("$argon2id$v=19$m=19456,t=2,p=1$fakehash")
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let other_id = Uuid::parse_str(&other_id_str).unwrap();
        let other_token =
            crate::auth::encode_token(other_id, false, "access", SECRET, 900).unwrap();

        let server_id_str: String = sqlx::query_scalar(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(admin_id.to_string())
        .bind(node_id.to_string())
        .bind("restart-forbidden")
        .bind("ubuntu")
        .bind(512)
        .bind(50)
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let server_id = Uuid::parse_str(&server_id_str).unwrap();

        let app = router(make_state(pool).await);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/servers/{}/restart", server_id))
            .header("authorization", format!("Bearer {}", other_token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }
}
