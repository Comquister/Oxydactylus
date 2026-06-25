# Server Status Tracking Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose and maintain the `status` field on servers so API clients can observe the real lifecycle state of each server.

**Architecture:** All changes are confined to `crates/panel/src/servers.rs`. No new migrations (the column already exists from migration 004), no new routes. The `Server` struct gains `pub status: String`; every SELECT picks it up; `create_server`, `start_server`, and `stop_server` write the appropriate value at each state transition.

**Tech Stack:** Rust 2021, axum 0.7, sqlx 0.8 (PostgreSQL), tonic 0.12, tokio

## Global Constraints

- Rust edition 2021, workspace resolver = "2"
- No new migrations — `status TEXT NOT NULL DEFAULT 'stopped' CHECK (status IN ('installing','running','stopped','error'))` already exists in migration 004
- All tests use `#[sqlx::test(migrations = "./migrations")]`
- Zero regressions in the 5 existing tests in `servers.rs`
- YAGNI: only what is in the spec; no new routes, no new files

---

### Task 1: Struct, SELECTs, and `create_server` lifecycle

**Files:**
- Modify: `crates/panel/src/servers.rs`

**Interfaces:**
- Consumes: nothing new — `AppState`, `sqlx::PgPool` as before
- Produces: `Server` struct now includes `pub status: String`; all SELECT queries return `status`; `create_server` returns a server with `status = "stopped"`

- [ ] **Step 1: Write the failing test**

Add the following test inside the `#[cfg(test)]` block in `crates/panel/src/servers.rs`, after the `create_server_provisions_on_node` test:

```rust
#[sqlx::test(migrations = "./migrations")]
async fn create_server_returns_stopped_status(pool: sqlx::PgPool) {
    let node_addr = start_mock_node("node-token").await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let (_, token) = seed_admin(&pool).await;
    let node_id = seed_node(&pool, &node_addr).await;

    let app = router(make_state(pool));
    let body = serde_json::json!({
        "node_id":     node_id,
        "name":        "status-test",
        "image":       "ubuntu",
        "memory_mb":   512,
        "cpu_percent": 50,
    });
    let req = Request::builder()
        .method("POST").uri("/api/servers")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let srv: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(srv["status"], "stopped");
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cargo test -p oxy-panel create_server_returns_stopped_status 2>&1 | tail -20
```

Expected: compile error (`unknown field 'status'` on `Server`) or test failure because `srv["status"]` is null. Either confirms the test is driving real change.

- [ ] **Step 3: Add `status` to the `Server` struct**

In `crates/panel/src/servers.rs`, find:

```rust
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
```

Replace with:

```rust
#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct Server {
    pub id:          Uuid,
    pub node_id:     Uuid,
    pub name:        String,
    pub image:       String,
    pub memory_mb:   i32,
    pub cpu_percent: i32,
    pub env:         Vec<String>,
    pub status:      String,
    pub created_at:  DateTime<Utc>,
}
```

- [ ] **Step 4: Update all SELECT queries to include `status`**

There are 9 SELECT queries in the file (`list_servers`, `get_server`, `delete_server`, `start_server`, `stop_server`, `provision_server`, `server_command`, `server_stats`, `stream_server_logs`). Every one currently has the column list ending with `env, created_at`.

Use your editor's find-and-replace across the whole file. Find:

```
id, node_id, name, image, memory_mb, cpu_percent, env, created_at
```

Replace with:

```
id, node_id, name, image, memory_mb, cpu_percent, env, status, created_at
```

Verify exactly 9 replacements were made (one per handler listed above).

- [ ] **Step 5: Update `create_server`'s INSERT to use `'installing'` and return `status`**

Find in `create_server`:

```rust
    let server = sqlx::query_as::<_, Server>(
        "INSERT INTO servers (node_id, name, image, memory_mb, cpu_percent, env, egg_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id, node_id, name, image, memory_mb, cpu_percent, env, created_at",
    )
```

Replace with:

```rust
    let mut server = sqlx::query_as::<_, Server>(
        "INSERT INTO servers (node_id, name, image, memory_mb, cpu_percent, env, egg_id, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'installing')
         RETURNING id, node_id, name, image, memory_mb, cpu_percent, env, status, created_at",
    )
```

Note: `let server` becomes `let mut server` so that `server.status` can be mutated in memory after provision.

- [ ] **Step 6: Add post-provision UPDATE and in-memory status mutation**

Find in `create_server` (the end of the function):

```rust
    if let Err(e) = client.provision(
        &server.id.to_string(),
        &server.image,
        server.memory_mb as u32,
        server.cpu_percent as u32,
        env,
    )
    .await {
        // compensating delete — best effort; ignore failure
        let _ = sqlx::query("DELETE FROM servers WHERE id = $1")
            .bind(server.id)
            .execute(&state.db)
            .await;
        return Err(e);
    }

    Ok((StatusCode::CREATED, Json(server)))
```

Replace with:

```rust
    if let Err(e) = client.provision(
        &server.id.to_string(),
        &server.image,
        server.memory_mb as u32,
        server.cpu_percent as u32,
        env,
    )
    .await {
        // compensating delete — best effort; ignore failure
        let _ = sqlx::query("DELETE FROM servers WHERE id = $1")
            .bind(server.id)
            .execute(&state.db)
            .await;
        return Err(e);
    }

    sqlx::query("UPDATE servers SET status = 'stopped' WHERE id = $1")
        .bind(server.id)
        .execute(&state.db)
        .await?;
    server.status = "stopped".to_string();

    Ok((StatusCode::CREATED, Json(server)))
```

- [ ] **Step 7: Run the test to verify it passes**

```bash
cargo test -p oxy-panel create_server_returns_stopped_status 2>&1 | tail -20
```

Expected: `test tests::create_server_returns_stopped_status ... ok`

- [ ] **Step 8: Run the full suite to confirm no regressions**

```bash
cargo test -p oxy-panel 2>&1 | tail -30
```

Expected: all previously passing tests still pass (DB-dependent tests are skipped when `DATABASE_URL` is not set; that's fine).

- [ ] **Step 9: Commit**

```bash
git add crates/panel/src/servers.rs
git commit -m "feat(servers): add status field + create_server installing→stopped lifecycle"
```

---

### Task 2: `start_server` and `stop_server` status transitions + tests

**Files:**
- Modify: `crates/panel/src/servers.rs`

**Interfaces:**
- Consumes: `Server.status: String` from Task 1; `AppState.db: sqlx::PgPool`
- Produces: `start_server` writes `'running'` on gRPC success, `'error'` on gRPC failure (best-effort); `stop_server` writes `'stopped'` on success, no change on failure

- [ ] **Step 1: Add `FailStartNode` mock and its helper to the test module**

Add the following inside the `#[cfg(test)]` block, after the `start_log_node` helper function:

```rust
struct FailStartNode;

#[async_trait]
impl NodeService for FailStartNode {
    type StreamLogsStream = ReceiverStream<std::result::Result<LogLine, Status>>;
    async fn provision_server(&self, _: GrpcRequest<ServerProvisionRequest>)
        -> std::result::Result<Response<ServerReply>, Status>
    { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
    async fn start_server(&self, _: GrpcRequest<ServerStartRequest>)
        -> std::result::Result<Response<ServerReply>, Status>
    { Err(Status::internal("docker error")) }
    async fn stop_server(&self, _: GrpcRequest<ServerStopRequest>)
        -> std::result::Result<Response<ServerReply>, Status>
    { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
    async fn delete_server(&self, _: GrpcRequest<ServerDeleteRequest>)
        -> std::result::Result<Response<ServerReply>, Status>
    { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
    async fn send_command(&self, _: GrpcRequest<ServerCommandRequest>)
        -> std::result::Result<Response<ServerReply>, Status>
    { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
    async fn get_stats(&self, _: GrpcRequest<ServerStatsRequest>)
        -> std::result::Result<Response<ServerStats>, Status>
    { Err(Status::internal("not implemented")) }
    async fn stream_logs(&self, _: GrpcRequest<ServerLogsRequest>)
        -> std::result::Result<Response<Self::StreamLogsStream>, Status>
    { let (_, rx) = tokio::sync::mpsc::channel(1); Ok(Response::new(ReceiverStream::new(rx))) }
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
```

- [ ] **Step 2: Write the three failing tests**

Add after `create_server_returns_stopped_status` in the test module:

```rust
#[sqlx::test(migrations = "./migrations")]
async fn start_server_sets_running_status(pool: sqlx::PgPool) {
    let node_addr = start_mock_node("node-token").await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let (_, token) = seed_admin(&pool).await;
    let node_id = seed_node(&pool, &node_addr).await;
    let server_id: Uuid = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO servers (node_id, name, image, memory_mb, cpu_percent)
         VALUES ($1,$2,$3,$4,$5) RETURNING id",
    )
    .bind(node_id).bind("start-srv").bind("ubuntu").bind(512).bind(50)
    .fetch_one(&pool).await.unwrap();

    let app = router(make_state(pool.clone()));
    let req = Request::builder()
        .method("POST").uri(format!("/api/servers/{}/start", server_id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let status: String = sqlx::query_scalar("SELECT status FROM servers WHERE id = $1")
        .bind(server_id)
        .fetch_one(&pool)
        .await.unwrap();
    assert_eq!(status, "running");
}

#[sqlx::test(migrations = "./migrations")]
async fn start_server_sets_error_on_node_failure(pool: sqlx::PgPool) {
    let node_addr = start_fail_node("node-token").await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let (_, token) = seed_admin(&pool).await;
    let node_id = seed_node(&pool, &node_addr).await;
    let server_id: Uuid = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO servers (node_id, name, image, memory_mb, cpu_percent)
         VALUES ($1,$2,$3,$4,$5) RETURNING id",
    )
    .bind(node_id).bind("fail-srv").bind("ubuntu").bind(512).bind(50)
    .fetch_one(&pool).await.unwrap();

    let app = router(make_state(pool.clone()));
    let req = Request::builder()
        .method("POST").uri(format!("/api/servers/{}/start", server_id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert!(res.status().is_server_error(), "expected 5xx, got {}", res.status());

    let status: String = sqlx::query_scalar("SELECT status FROM servers WHERE id = $1")
        .bind(server_id)
        .fetch_one(&pool)
        .await.unwrap();
    assert_eq!(status, "error");
}

#[sqlx::test(migrations = "./migrations")]
async fn stop_server_sets_stopped_status(pool: sqlx::PgPool) {
    let node_addr = start_mock_node("node-token").await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let (_, token) = seed_admin(&pool).await;
    let node_id = seed_node(&pool, &node_addr).await;
    let server_id: Uuid = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO servers (node_id, name, image, memory_mb, cpu_percent)
         VALUES ($1,$2,$3,$4,$5) RETURNING id",
    )
    .bind(node_id).bind("stop-srv").bind("ubuntu").bind(512).bind(50)
    .fetch_one(&pool).await.unwrap();

    sqlx::query("UPDATE servers SET status = 'running' WHERE id = $1")
        .bind(server_id)
        .execute(&pool).await.unwrap();

    let app = router(make_state(pool.clone()));
    let req = Request::builder()
        .method("POST").uri(format!("/api/servers/{}/stop", server_id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let status: String = sqlx::query_scalar("SELECT status FROM servers WHERE id = $1")
        .bind(server_id)
        .fetch_one(&pool)
        .await.unwrap();
    assert_eq!(status, "stopped");
}
```

- [ ] **Step 3: Run the tests to verify they fail**

```bash
cargo test -p oxy-panel "start_server_sets_running_status|start_server_sets_error_on_node_failure|stop_server_sets_stopped_status" 2>&1 | tail -30
```

Expected: tests compile but fail — `status` stays `'stopped'` (default) instead of the expected value, because the handlers don't UPDATE yet.

- [ ] **Step 4: Update `start_server` to write `'running'` on success and `'error'` on gRPC failure**

Find in `crates/panel/src/servers.rs`:

```rust
    let mut client = get_node_client(&state, server.node_id).await?;
    client.start(&server.id.to_string()).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

(This is the closing section of `start_server`. Replace only these 3 lines, not the whole function.)

Replace with:

```rust
    let mut client = get_node_client(&state, server.node_id).await?;
    match client.start(&server.id.to_string()).await {
        Ok(_) => {
            sqlx::query("UPDATE servers SET status = 'running' WHERE id = $1")
                .bind(server.id)
                .execute(&state.db)
                .await?;
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            let _ = sqlx::query("UPDATE servers SET status = 'error' WHERE id = $1")
                .bind(server.id)
                .execute(&state.db)
                .await;
            Err(e)
        }
    }
}
```

The `let _ = ...` in the `Err` branch is intentional: the UPDATE is best-effort so that a DB failure doesn't mask the original gRPC error.

- [ ] **Step 5: Update `stop_server` to write `'stopped'` on success**

Find in `crates/panel/src/servers.rs`:

```rust
    let mut client = get_node_client(&state, server.node_id).await?;
    client.stop(&server.id.to_string(), 10).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

(This is the closing section of `stop_server`.)

Replace with:

```rust
    let mut client = get_node_client(&state, server.node_id).await?;
    client.stop(&server.id.to_string(), 10).await?;
    sqlx::query("UPDATE servers SET status = 'stopped' WHERE id = $1")
        .bind(server.id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
```

- [ ] **Step 6: Run the three new tests to verify they pass**

```bash
cargo test -p oxy-panel "start_server_sets_running_status|start_server_sets_error_on_node_failure|stop_server_sets_stopped_status" 2>&1 | tail -30
```

Expected: all 3 pass.

- [ ] **Step 7: Run the full test suite to confirm zero regressions**

```bash
cargo test -p oxy-panel 2>&1 | tail -30
```

Expected: all previously passing tests still pass.

- [ ] **Step 8: Commit**

```bash
git add crates/panel/src/servers.rs
git commit -m "feat(servers): start/stop status transitions + lifecycle tests"
```
