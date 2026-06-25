use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::{AdminUser, AuthUser},
    error::{PanelError, Result},
    node_client::NodeClient,
    AppState,
};

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct Server {
    pub id:          Uuid,
    pub node_id:     Uuid,
    pub name:        String,
    pub image:       String,
    pub memory_mb:   i32,
    pub cpu_percent: i32,
    pub env:         Vec<String>,
    pub created_at:  DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct NodeRow {
    grpc_addr: String,
    token:     String,
}

async fn get_node_client(state: &AppState, node_id: Uuid) -> Result<NodeClient> {
    let row = sqlx::query_as::<_, NodeRow>(
        "SELECT grpc_addr, token FROM nodes WHERE id = $1",
    )
    .bind(node_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| PanelError::NotFound(format!("node {}", node_id)))?;
    NodeClient::connect(&row.grpc_addr, &row.token).await
}

async fn list_servers(
    State(state): State<AppState>,
    _user: AuthUser,
) -> Result<Json<Vec<Server>>> {
    let servers = sqlx::query_as::<_, Server>(
        "SELECT id, node_id, name, image, memory_mb, cpu_percent, env, created_at
         FROM servers ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(servers))
}

#[derive(Debug, Deserialize)]
struct CreateServerRequest {
    node_id:     Uuid,
    name:        String,
    image:       String,
    memory_mb:   i32,
    cpu_percent: i32,
    #[serde(default)]
    env:         Vec<String>,
}

async fn create_server(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<CreateServerRequest>,
) -> Result<(StatusCode, Json<Server>)> {
    if body.memory_mb <= 0 || body.cpu_percent <= 0 {
        return Err(PanelError::Validation(
            "memory_mb and cpu_percent must be positive".to_string(),
        ));
    }
    if body.name.is_empty() || body.image.is_empty() {
        return Err(PanelError::Validation("name and image are required".to_string()));
    }

    let server = sqlx::query_as::<_, Server>(
        "INSERT INTO servers (node_id, name, image, memory_mb, cpu_percent, env)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id, node_id, name, image, memory_mb, cpu_percent, env, created_at",
    )
    .bind(body.node_id)
    .bind(&body.name)
    .bind(&body.image)
    .bind(body.memory_mb)
    .bind(body.cpu_percent)
    .bind(&body.env)
    .fetch_one(&state.db)
    .await?;

    let mut client = get_node_client(&state, server.node_id).await?;
    client.provision(
        &server.id.to_string(),
        &server.image,
        server.memory_mb as u32,
        server.cpu_percent as u32,
        server.env.clone(),
    )
    .await?;

    Ok((StatusCode::CREATED, Json(server)))
}

async fn get_server(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Server>> {
    let server = sqlx::query_as::<_, Server>(
        "SELECT id, node_id, name, image, memory_mb, cpu_percent, env, created_at
         FROM servers WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(server))
}

async fn delete_server(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let server = sqlx::query_as::<_, Server>(
        "SELECT id, node_id, name, image, memory_mb, cpu_percent, env, created_at
         FROM servers WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    if let Ok(mut client) = get_node_client(&state, server.node_id).await {
        let _ = client.stop(&server.id.to_string(), 10).await;
        let _ = client.delete(&server.id.to_string()).await;
    }

    sqlx::query("DELETE FROM servers WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn start_server(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let server = sqlx::query_as::<_, Server>(
        "SELECT id, node_id, name, image, memory_mb, cpu_percent, env, created_at
         FROM servers WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    let mut client = get_node_client(&state, server.node_id).await?;
    client.start(&server.id.to_string()).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn stop_server(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let server = sqlx::query_as::<_, Server>(
        "SELECT id, node_id, name, image, memory_mb, cpu_percent, env, created_at
         FROM servers WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    let mut client = get_node_client(&state, server.node_id).await?;
    client.stop(&server.id.to_string(), 10).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn provision_server(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let server = sqlx::query_as::<_, Server>(
        "SELECT id, node_id, name, image, memory_mb, cpu_percent, env, created_at
         FROM servers WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    let mut client = get_node_client(&state, server.node_id).await?;
    client.provision(
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
    _user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<StatusCode> {
    let content = body["content"]
        .as_str()
        .ok_or_else(|| PanelError::Validation("content field required".to_string()))?
        .to_string();
    let server = sqlx::query_as::<_, Server>(
        "SELECT id, node_id, name, image, memory_mb, cpu_percent, env, created_at
         FROM servers WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    let mut client = get_node_client(&state, server.node_id).await?;
    client.send_command(&server.id.to_string(), &content).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn server_stats(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    let server = sqlx::query_as::<_, Server>(
        "SELECT id, node_id, name, image, memory_mb, cpu_percent, env, created_at
         FROM servers WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    let mut client = get_node_client(&state, server.node_id).await?;
    let stats = client.get_stats(&server.id.to_string()).await?;
    Ok(Json(serde_json::json!({
        "memory_bytes": stats.memory_bytes,
        "cpu_percent":  stats.cpu_percent,
        "rx_bytes":     stats.rx_bytes,
        "tx_bytes":     stats.tx_bytes,
    })))
}

pub fn servers_router() -> Router<AppState> {
    Router::new()
        .route("/",              get(list_servers).post(create_server))
        .route("/:id",           get(get_server).delete(delete_server))
        .route("/:id/start",     post(start_server))
        .route("/:id/stop",      post(stop_server))
        .route("/:id/provision", post(provision_server))
        .route("/:id/command",   post(server_command))
        .route("/:id/stats",     get(server_stats))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        auth::{encode_token, hash_password},
        router, AppState,
    };
    use axum::{body::Body, http::{Request, StatusCode}};
    use http_body_util::BodyExt;
    use oxy_core::proto::node::{
        node_service_server::{NodeService, NodeServiceServer},
        LogLine, ServerCommandRequest, ServerDeleteRequest, ServerLogsRequest,
        ServerProvisionRequest, ServerReply, ServerStartRequest, ServerStatsRequest,
        ServerStats, ServerStopRequest,
    };
    use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
    use tonic::{async_trait, Request as GrpcRequest, Response, Status};
    use tower::ServiceExt;
    use uuid::Uuid;

    const SECRET: &str = "test-secret-at-least-32-chars-long!!";

    fn make_state(pool: sqlx::PgPool) -> AppState {
        AppState { db: pool, jwt_secret: SECRET.to_string() }
    }

    async fn seed_admin(pool: &sqlx::PgPool) -> (Uuid, String) {
        let id = Uuid::new_v4();
        let hash = hash_password("pass").unwrap();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, is_admin) VALUES ($1, $2, $3, $4)",
        )
        .bind(id).bind("a@t.com").bind(&hash).bind(true)
        .execute(pool).await.unwrap();
        let token = encode_token(id, true, "access", SECRET, 900).unwrap();
        (id, token)
    }

    struct AcceptAllNode;

    #[async_trait]
    impl NodeService for AcceptAllNode {
        type StreamLogsStream = ReceiverStream<std::result::Result<LogLine, Status>>;
        async fn provision_server(&self, _: GrpcRequest<ServerProvisionRequest>)
            -> std::result::Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
        async fn start_server(&self, _: GrpcRequest<ServerStartRequest>)
            -> std::result::Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
        async fn stop_server(&self, _: GrpcRequest<ServerStopRequest>)
            -> std::result::Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
        async fn delete_server(&self, _: GrpcRequest<ServerDeleteRequest>)
            -> std::result::Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
        async fn send_command(&self, _: GrpcRequest<ServerCommandRequest>)
            -> std::result::Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
        async fn get_stats(&self, req: GrpcRequest<ServerStatsRequest>)
            -> std::result::Result<Response<ServerStats>, Status>
        {
            Ok(Response::new(ServerStats {
                server_id: req.into_inner().server_id,
                memory_bytes: 1024, cpu_percent: 10.0,
                rx_bytes: 50, tx_bytes: 100,
            }))
        }
        async fn stream_logs(&self, _: GrpcRequest<ServerLogsRequest>)
            -> std::result::Result<Response<Self::StreamLogsStream>, Status>
        { let (_, rx) = tokio::sync::mpsc::channel(1); Ok(Response::new(ReceiverStream::new(rx))) }
    }

    async fn start_mock_node(token: &str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let t = token.to_string();
        tokio::spawn(async move {
            use oxy_node::interceptor::AuthInterceptor;
            tonic::transport::Server::builder()
                .add_service(NodeServiceServer::with_interceptor(AcceptAllNode, AuthInterceptor::new(&t)))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        });
        format!("http://127.0.0.1:{}", port)
    }

    async fn seed_node(pool: &sqlx::PgPool, grpc_addr: &str) -> Uuid {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO nodes (name, grpc_addr, token) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind("test-node").bind(grpc_addr).bind("node-token")
        .fetch_one(pool).await.unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_server_provisions_on_node(pool: sqlx::PgPool) {
        let node_addr = start_mock_node("node-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (_, token) = seed_admin(&pool).await;
        let node_id = seed_node(&pool, &node_addr).await;

        let app = router(make_state(pool));
        let body = serde_json::json!({
            "node_id":     node_id,
            "name":        "mc-server-1",
            "image":       "itzg/minecraft-server",
            "memory_mb":   1024,
            "cpu_percent": 100,
            "env":         ["EULA=TRUE"]
        });
        let req = Request::builder()
            .method("POST").uri("/api/servers")
            .header("authorization", format!("Bearer {}", token))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn list_servers_returns_empty(pool: sqlx::PgPool) {
        let (_, token) = seed_admin(&pool).await;
        let app = router(make_state(pool));
        let req = Request::builder()
            .method("GET").uri("/api/servers")
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty()).unwrap();
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
        let (_, token) = seed_admin(&pool).await;
        let node_id = seed_node(&pool, &node_addr).await;
        // Insert server directly
        let server_id: Uuid = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO servers (node_id, name, image, memory_mb, cpu_percent)
             VALUES ($1, $2, $3, $4, $5) RETURNING id",
        )
        .bind(node_id).bind("srv-x").bind("ubuntu").bind(512).bind(50)
        .fetch_one(&pool).await.unwrap();

        let app = router(make_state(pool));
        let req = Request::builder()
            .method("GET").uri(format!("/api/servers/{}/stats", server_id))
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["memory_bytes"], 1024);
    }
}
