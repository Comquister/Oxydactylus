# Server Log Streaming Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose the node's `StreamLogs` gRPC streaming call through the panel as `GET /api/servers/:id/logs?follow=<bool>`, delivering log lines as Server-Sent Events.

**Architecture:** Two targeted changes. `NodeClient` gains a `stream_logs` method that returns a `Pin<Box<dyn Stream<Item = Result<LogLine, PanelError>> + Send>>` backed by the tonic `Streaming<LogLine>`. A new handler in `servers.rs` calls it and wraps the result in axum's `Sse<_>` response type. Setup failures (server not found, node unreachable) surface as HTTP errors; per-line gRPC errors become `event: error` SSE messages once streaming has started.

**Tech Stack:** Rust, axum 0.7 (`axum::response::sse`), `futures-util 0.3` (added to panel deps), `tonic::Streaming<T>`, `tokio-stream` (already in dev-deps)

## Global Constraints

- Rust edition 2021, workspace resolver = "2"
- All handlers return `crate::error::Result<T>`; SQL errors convert via `impl From<sqlx::Error> for PanelError`
- Authenticated routes use `AuthUser` extractor; admin-only routes use `AdminUser`
- Tests use `#[sqlx::test(migrations = "./migrations")]` so all 4 migrations run automatically
- Run `cargo test -p oxy-panel` after every task; zero regressions required
- YAGNI: implement only what the spec defines

---

### Task 1: `NodeClient::stream_logs`

**Files:**
- Modify: `crates/panel/src/node_client.rs`
- Modify: `crates/panel/Cargo.toml`

**Interfaces:**
- Consumes: `tonic::Streaming<LogLine>` from the gRPC `StreamLogs` call
- Produces:
  ```rust
  pub async fn stream_logs(
      &mut self,
      server_id: &str,
      follow: bool,
  ) -> Result<Pin<Box<dyn Stream<Item = Result<LogLine, PanelError>> + Send>>>
  ```

- [ ] **Step 1: Add `futures-util` to panel's regular dependencies**

In `crates/panel/Cargo.toml`, add to `[dependencies]`:

```toml
futures-util = { workspace = true }
```

- [ ] **Step 2: Write the failing test**

In `crates/panel/src/node_client.rs`, inside the existing `tests` module, add a `LogNode` struct that sends two log lines, then a test that exercises `stream_logs`:

```rust
    #[tokio::test]
    async fn client_stream_logs_yields_lines() {
        use futures_util::StreamExt;

        struct LogNode;

        #[async_trait]
        impl NodeService for LogNode {
            type StreamLogsStream = ReceiverStream<std::result::Result<LogLine, Status>>;

            async fn provision_server(&self, _: Request<ServerProvisionRequest>)
                -> std::result::Result<Response<ServerReply>, Status>
            { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }

            async fn start_server(&self, _: Request<ServerStartRequest>)
                -> std::result::Result<Response<ServerReply>, Status>
            { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }

            async fn stop_server(&self, _: Request<ServerStopRequest>)
                -> std::result::Result<Response<ServerReply>, Status>
            { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }

            async fn delete_server(&self, _: Request<ServerDeleteRequest>)
                -> std::result::Result<Response<ServerReply>, Status>
            { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }

            async fn send_command(&self, _: Request<ServerCommandRequest>)
                -> std::result::Result<Response<ServerReply>, Status>
            { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }

            async fn get_stats(&self, req: Request<ServerStatsRequest>)
                -> std::result::Result<Response<ServerStats>, Status>
            {
                Ok(Response::new(ServerStats {
                    server_id: req.into_inner().server_id,
                    memory_bytes: 0, cpu_percent: 0.0, rx_bytes: 0, tx_bytes: 0,
                }))
            }

            async fn stream_logs(&self, _: Request<ServerLogsRequest>)
                -> std::result::Result<Response<Self::StreamLogsStream>, Status>
            {
                let (tx, rx) = tokio::sync::mpsc::channel(4);
                tokio::spawn(async move {
                    let _ = tx.send(Ok(LogLine {
                        content: "hello\n".into(), stream: "stdout".into(), timestamp: 0,
                    })).await;
                    let _ = tx.send(Ok(LogLine {
                        content: "world\n".into(), stream: "stdout".into(), timestamp: 0,
                    })).await;
                });
                Ok(Response::new(ReceiverStream::new(rx)))
            }
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let token = "log-token";
        let t = token.to_string();
        tokio::spawn(async move {
            use oxy_node::interceptor::AuthInterceptor;
            use oxy_core::proto::node::node_service_server::NodeServiceServer;
            use tokio_stream::wrappers::TcpListenerStream;
            tonic::transport::Server::builder()
                .add_service(NodeServiceServer::with_interceptor(
                    LogNode,
                    AuthInterceptor::new(&t),
                ))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let addr = format!("http://127.0.0.1:{}", port);
        let mut client = NodeClient::connect(&addr, token).await.unwrap();
        let mut stream = client.stream_logs("srv-1", false).await.unwrap();

        let line1 = stream.next().await.unwrap().unwrap();
        assert_eq!(line1.content, "hello\n");
        let line2 = stream.next().await.unwrap().unwrap();
        assert_eq!(line2.content, "world\n");
        assert!(stream.next().await.is_none());
    }
```

- [ ] **Step 3: Run — expect compile failure**

```bash
cargo test -p oxy-panel node_client 2>&1 | head -20
```

Expected: compile error — method `stream_logs` not found on `NodeClient`.

- [ ] **Step 4: Add `stream_logs` to `node_client.rs`**

Update the existing proto import to include `ServerLogsRequest`:

```rust
use oxy_core::proto::node::{
    node_service_client::NodeServiceClient,
    LogLine, ServerCommandRequest, ServerDeleteRequest, ServerLogsRequest,
    ServerProvisionRequest, ServerStartRequest, ServerStats, ServerStatsRequest,
    ServerStopRequest,
};
```

Add new top-level imports:

```rust
use futures_util::{Stream, StreamExt};
use std::pin::Pin;
```

Add the method to `NodeClient`'s `impl` block, after `get_stats`:

```rust
    pub async fn stream_logs(
        &mut self,
        server_id: &str,
        follow: bool,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<LogLine, PanelError>> + Send>>> {
        let streaming = self
            .inner
            .stream_logs(ServerLogsRequest {
                server_id: server_id.to_string(),
                follow,
            })
            .await
            .map_err(PanelError::from)?
            .into_inner();
        Ok(Box::pin(streaming.map(|r| r.map_err(PanelError::from))))
    }
```

- [ ] **Step 5: Run tests — expect pass**

```bash
cargo test -p oxy-panel node_client 2>&1 | tail -10
```

Expected: all existing node_client tests pass plus `client_stream_logs_yields_lines`.

- [ ] **Step 6: Commit**

```bash
git add crates/panel/src/node_client.rs crates/panel/Cargo.toml
git commit -m "feat(oxy-panel): NodeClient::stream_logs — gRPC streaming passthrough"
```

---

### Task 2: SSE log-streaming endpoint

**Files:**
- Modify: `crates/panel/src/servers.rs`

**Interfaces:**
- Consumes: `NodeClient::stream_logs` (Task 1); `AuthUser` extractor; `AppState`
- Produces:
  - Route: `GET /api/servers/:id/logs?follow=<bool>` (defaults `follow` to `false`)
  - Response: `Content-Type: text/event-stream`
  - Per `LogLine`: `event: <stream>\ndata: <content without trailing newline>`
  - On gRPC error mid-stream: `event: error\ndata: <message>`

- [ ] **Step 1: Write the failing test**

Add to the existing `tests` module inside `crates/panel/src/servers.rs`.

Add `LogNode` struct (a separate node impl that actually emits log lines) and `start_log_node` helper, then the test:

```rust
    struct LogNode;

    #[async_trait]
    impl NodeService for LogNode {
        type StreamLogsStream = ReceiverStream<Result<LogLine, Status>>;

        async fn provision_server(&self, _: GrpcRequest<ServerProvisionRequest>)
            -> Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
        async fn start_server(&self, _: GrpcRequest<ServerStartRequest>)
            -> Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
        async fn stop_server(&self, _: GrpcRequest<ServerStopRequest>)
            -> Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
        async fn delete_server(&self, _: GrpcRequest<ServerDeleteRequest>)
            -> Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
        async fn send_command(&self, _: GrpcRequest<ServerCommandRequest>)
            -> Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
        async fn get_stats(&self, req: GrpcRequest<ServerStatsRequest>)
            -> Result<Response<ServerStats>, Status>
        {
            Ok(Response::new(ServerStats {
                server_id: req.into_inner().server_id,
                memory_bytes: 0, cpu_percent: 0.0, rx_bytes: 0, tx_bytes: 0,
            }))
        }
        async fn stream_logs(&self, _: GrpcRequest<ServerLogsRequest>)
            -> Result<Response<Self::StreamLogsStream>, Status>
        {
            let (tx, rx) = tokio::sync::mpsc::channel(4);
            tokio::spawn(async move {
                let _ = tx.send(Ok(LogLine {
                    content: "starting up\n".into(), stream: "stdout".into(), timestamp: 0,
                })).await;
                let _ = tx.send(Ok(LogLine {
                    content: "ready\n".into(), stream: "stdout".into(), timestamp: 0,
                })).await;
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

    #[sqlx::test(migrations = "./migrations")]
    async fn stream_logs_returns_sse_events(pool: sqlx::PgPool) {
        let node_addr = start_log_node("node-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (_, token) = seed_admin(&pool).await;
        let node_id = seed_node(&pool, &node_addr).await;

        let server_id: Uuid = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO servers (node_id, name, image, memory_mb, cpu_percent)
             VALUES ($1,$2,$3,$4,$5) RETURNING id",
        )
        .bind(node_id).bind("log-srv").bind("ubuntu").bind(512).bind(50)
        .fetch_one(&pool).await.unwrap();

        let app = router(make_state(pool));
        let req = Request::builder()
            .method("GET")
            .uri(format!("/api/servers/{}/logs", server_id))
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();

        assert_eq!(res.status(), StatusCode::OK);
        let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("text/event-stream"), "expected SSE, got {}", ct);

        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("data:"), "SSE body missing data lines:\n{}", body);
        assert!(body.contains("starting up"), "first log line missing:\n{}", body);
    }
```

- [ ] **Step 2: Run — expect compile failure**

```bash
cargo test -p oxy-panel servers 2>&1 | head -20
```

Expected: compile error — `stream_server_logs` handler not defined.

- [ ] **Step 3: Add imports, query struct, and handler to `servers.rs`**

Update the `axum` import block at the top of `servers.rs` to include `Query` and SSE types:

```rust
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    routing::{delete, get, post},
    Json, Router,
};
```

Add `futures_util` and `Infallible` imports:

```rust
use futures_util::StreamExt;
use std::convert::Infallible;
```

Add the query struct after `CreateServerRequest`:

```rust
#[derive(Debug, Deserialize)]
struct LogsQuery {
    #[serde(default)]
    follow: bool,
}
```

Add the handler after `server_stats`:

```rust
async fn stream_server_logs(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<LogsQuery>,
) -> Result<Sse<impl futures_util::Stream<Item = std::result::Result<Event, Infallible>> + Send>> {
    let server = sqlx::query_as::<_, Server>(
        "SELECT id, node_id, name, image, memory_mb, cpu_percent, env, created_at
         FROM servers WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    let mut client = get_node_client(&state, server.node_id).await?;
    let log_stream = client.stream_logs(&server.id.to_string(), q.follow).await?;

    let sse_stream = log_stream.map(|result| {
        let event = match result {
            Ok(line) => Event::default()
                .event(line.stream)
                .data(line.content.trim_end_matches('\n')),
            Err(e) => Event::default().event("error").data(e.to_string()),
        };
        Ok::<Event, Infallible>(event)
    });

    Ok(Sse::new(sse_stream))
}
```

- [ ] **Step 4: Mount the route in `servers_router()`**

```rust
pub fn servers_router() -> Router<AppState> {
    Router::new()
        .route("/",            get(list_servers).post(create_server))
        .route("/:id",         get(get_server).delete(delete_server))
        .route("/:id/start",   post(start_server))
        .route("/:id/stop",    post(stop_server))
        .route("/:id/command", post(server_command))
        .route("/:id/stats",   get(server_stats))
        .route("/:id/logs",    get(stream_server_logs))
}
```

- [ ] **Step 5: Run all tests — expect pass**

```bash
cargo test -p oxy-panel 2>&1 | tail -15
```

Expected: all prior tests still pass; `stream_logs_returns_sse_events` passes.

- [ ] **Step 6: Run full workspace build**

```bash
cargo build --workspace 2>&1
```

Expected: clean build, no errors.

- [ ] **Step 7: Commit**

```bash
git add crates/panel/src/servers.rs
git commit -m "feat(oxy-panel): SSE log-streaming endpoint GET /api/servers/:id/logs"
```
