use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use oxy_core::proto::node::{
    node_service_server::{NodeService, NodeServiceServer},
    LogLine, ServerCommandRequest, ServerDeleteRequest, ServerLogsRequest, ServerProvisionRequest,
    ServerReply, ServerStartRequest, ServerStats, ServerStatsRequest, ServerStopRequest,
};
use oxy_panel::{
    auth::{encode_token, hash_password},
    router, AppState,
};
use sqlx::PgPool;
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
use tonic::{async_trait, Request as GrpcRequest, Response, Status};
use tower::ServiceExt;
use uuid::Uuid;

const SECRET: &str = "integration-test-secret-32-chars!!";

struct OkNode;

#[async_trait]
impl NodeService for OkNode {
    type StreamLogsStream = ReceiverStream<Result<LogLine, Status>>;
    async fn provision_server(
        &self,
        _: GrpcRequest<ServerProvisionRequest>,
    ) -> Result<Response<ServerReply>, Status> {
        Ok(Response::new(ServerReply {
            success: true,
            message: "ok".into(),
        }))
    }
    async fn start_server(
        &self,
        _: GrpcRequest<ServerStartRequest>,
    ) -> Result<Response<ServerReply>, Status> {
        Ok(Response::new(ServerReply {
            success: true,
            message: "ok".into(),
        }))
    }
    async fn stop_server(
        &self,
        _: GrpcRequest<ServerStopRequest>,
    ) -> Result<Response<ServerReply>, Status> {
        Ok(Response::new(ServerReply {
            success: true,
            message: "ok".into(),
        }))
    }
    async fn delete_server(
        &self,
        _: GrpcRequest<ServerDeleteRequest>,
    ) -> Result<Response<ServerReply>, Status> {
        Ok(Response::new(ServerReply {
            success: true,
            message: "ok".into(),
        }))
    }
    async fn send_command(
        &self,
        _: GrpcRequest<ServerCommandRequest>,
    ) -> Result<Response<ServerReply>, Status> {
        Ok(Response::new(ServerReply {
            success: true,
            message: "ok".into(),
        }))
    }
    async fn get_stats(
        &self,
        req: GrpcRequest<ServerStatsRequest>,
    ) -> Result<Response<ServerStats>, Status> {
        Ok(Response::new(ServerStats {
            server_id: req.into_inner().server_id,
            memory_bytes: 256,
            cpu_percent: 3.0,
            rx_bytes: 10,
            tx_bytes: 20,
        }))
    }
    async fn stream_logs(
        &self,
        _: GrpcRequest<ServerLogsRequest>,
    ) -> Result<Response<Self::StreamLogsStream>, Status> {
        let (_, rx) = tokio::sync::mpsc::channel(1);
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

async fn start_node(token: &str) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let t = token.to_string();
    tokio::spawn(async move {
        use oxy_node::interceptor::AuthInterceptor;
        tonic::transport::Server::builder()
            .add_service(NodeServiceServer::with_interceptor(
                OkNode,
                AuthInterceptor::new(&t),
            ))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    format!("http://127.0.0.1:{}", port)
}

fn auth_header(id: Uuid, admin: bool) -> String {
    format!(
        "Bearer {}",
        encode_token(id, admin, "access", SECRET, 900).unwrap()
    )
}

#[sqlx::test(migrations = "./migrations")]
async fn full_panel_flow(pool: PgPool) {
    let node_addr = start_node("node-secret").await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let state = AppState {
        db: pool.clone(),
        jwt_secret: SECRET.to_string(),
    };
    let app = router(state);

    // 1. Create admin user
    let admin_id = Uuid::new_v4();
    let hash = hash_password("admin-pass").unwrap();
    sqlx::query("INSERT INTO users (id, email, password_hash, is_admin) VALUES ($1, $2, $3, $4)")
        .bind(admin_id)
        .bind("admin@example.com")
        .bind(&hash)
        .bind(true)
        .execute(&pool)
        .await
        .unwrap();

    // 2. Login
    let login_body = serde_json::json!({
        "email": "admin@example.com", "password": "admin-pass"
    });
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&login_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let tokens: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let access_token = tokens["access_token"].as_str().unwrap().to_string();
    let bearer = format!("Bearer {}", access_token);

    // 3. Create node
    let node_body = serde_json::json!({
        "name": "eu-1", "grpc_addr": node_addr, "token": "node-secret"
    });
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/nodes")
                .header("authorization", &bearer)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&node_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let node: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let node_id = node["id"].as_str().unwrap();

    // 4. Create + provision server
    let server_body = serde_json::json!({
        "node_id": node_id, "name": "mc-1",
        "image": "itzg/minecraft-server",
        "memory_mb": 1024, "cpu_percent": 100,
        "env": ["EULA=TRUE"]
    });
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/servers")
                .header("authorization", &bearer)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&server_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let server: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let server_id = server["id"].as_str().unwrap();

    // 5. Get stats
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/servers/{}/stats", server_id))
                .header("authorization", &bearer)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let stats: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(stats["memory_bytes"], 256u64);

    // 6. Delete server (stop + delete on node + DB)
    let res = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/servers/{}", server_id))
                .header("authorization", &bearer)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

// Verify auth_header helper compiles (used for manual token construction if needed)
#[allow(dead_code)]
fn _uses_auth_header() {
    let _ = auth_header(Uuid::new_v4(), true);
}
