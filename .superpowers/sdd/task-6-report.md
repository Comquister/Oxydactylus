# Task 6 Report: NodeClient gRPC Wrapper

## Status: DONE

## Commits
- `8f10994` feat(oxy-panel): NodeClient gRPC wrapper with bearer token interceptor

## What Was Created/Modified

### Created: `crates/panel/src/node_client.rs`
- `BearerInterceptor` — local `tonic::service::Interceptor` that injects `Authorization: Bearer <token>` into every outgoing request's gRPC metadata
- `NodeClient` — wraps `NodeServiceClient<InterceptedService<Channel, BearerInterceptor>>`
- `NodeClient::connect(grpc_addr: &str, token: &str) -> Result<NodeClient>` — creates a connected channel via `Channel::from_shared().connect().await`, wraps it with the interceptor
- Methods (all `&mut self`): `provision`, `start`, `stop`, `delete`, `send_command`, `get_stats` — all return `Result<()>` or `Result<ServerStats>`; tonic errors are converted to `PanelError::Node` via the existing `From<tonic::Status>` impl
- 3 integration tests using an in-process `EchoNode` mock server with `oxy_node::interceptor::AuthInterceptor` for server-side auth validation

### Modified: `crates/panel/src/lib.rs`
- Added `pub mod node_client;`

### Modified: `crates/panel/Cargo.toml`
- Added `[dev-dependencies]`:
  - `oxy-node = { path = "../node" }` — for `AuthInterceptor` in test server
  - `tokio-stream = { workspace = true, features = ["net"] }` — for `TcpListenerStream`

## Test Command and Output

```
$ cargo test -p oxy-panel node_client 2>&1

running 3 tests
test node_client::tests::client_gets_stats ... ok
test node_client::tests::client_can_provision_and_start ... ok
test node_client::tests::wrong_token_returns_node_error ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 19 filtered out; finished in 0.06s
```

Full suite `cargo test -p oxy-panel`: 14 tests pass, 8 pre-existing DB tests fail without `DATABASE_URL` (unchanged from prior tasks).

## Self-Review Findings

1. **`sqlx = { workspace = true, features = ["test"] }` doesn't exist** — the brief included `sqlx` with `features = ["test"]` in dev-dependencies, but SQLx 0.8 has no `test` feature (it uses `#[sqlx::test]` via the `sqlx-test` macro, not a feature flag). Removed that line; existing DB tests continue to work as before.

2. **`futures_util::future::FutureExt` import not needed** — the brief's test template imported it but it's unused. Omitted to avoid compiler warnings.

3. **No `unwrap()`/`expect()` in non-test code** — verified; all error paths use `?` with `PanelError::Node`.

4. **`BearerInterceptor` defined locally instead of importing from `oxy-node`** — the brief specifies a local definition (the node crate's `AuthInterceptor` is a server-side interceptor for validating incoming tokens; the panel-side interceptor attaches outgoing tokens). This is intentional and correct.

## Fix: NodeClient::new(node)
- Added `pub async fn new(node: &Node) -> Result<Self>` to NodeClient
- Wraps `connect()` for convenient client creation from a `Node` struct
- Tests: 3 tests passing (client_can_provision_and_start, client_gets_stats, wrong_token_returns_node_error)
