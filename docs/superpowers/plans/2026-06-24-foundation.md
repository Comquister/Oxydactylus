# Oxydactylus Foundation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Scaffold the Cargo workspace, define shared types (config, errors, proto) in `oxy-core`, create minimal compiling stubs for `oxy-node` and `oxy-panel`, and wire up the `oxydactylus` binary that reads `config.toml` and dispatches to the correct role.

**Architecture:** Single Cargo workspace with 4 crates: `oxy-core` (shared types, proto), `oxy-node` (daemon stub), `oxy-panel` (web stub), `oxydactylus` CLI (entrypoint). The CLI binary reads `config.toml`, parses the `[role]` section, and calls `oxy_node::run()` or `oxy_panel::run()` — each currently logs and pends indefinitely. Proto definitions live in `oxy-core` and are compiled by `tonic-build` via `build.rs`; all other crates import types via `oxy_core::proto::*`.

**Tech Stack:** Rust stable, tokio 1.x, tonic 0.12, prost 0.13, tonic-build 0.12, clap 4, serde 1 + toml 0.8, thiserror 2, anyhow 1, tracing 0.1 + tracing-subscriber 0.3

## Global Constraints

- Rust edition: 2021
- All async via tokio with `features = ["full"]`
- No `unwrap()` in library code — use `?` with typed errors or `expect()` with a clear message at startup boundaries only
- No `println!` in library code — use `tracing::{info, warn, error}` macros
- Package names: `oxy-core`, `oxy-node`, `oxy-panel`; binary name: `oxydactylus`
- `protoc` (protobuf compiler) must be installed on the build machine — `apt install protobuf-compiler` or `brew install protobuf`
- `core` as a Rust import alias is avoided — use the full crate name `oxy_core`

---

## File Map

```
Cargo.toml                          ← workspace root
rust-toolchain.toml                 ← pin stable channel
config.example.toml                 ← example config for all three roles
crates/
  core/                             ← package: oxy-core
    Cargo.toml
    build.rs                        ← tonic-build: compila .proto → OUT_DIR
    proto/
      oxydactylus.proto             ← NodeService, all messages
    src/
      lib.rs                        ← pub re-exports
      config.rs                     ← Config, Role, PanelConfig, NodeConfig
      error.rs                      ← OxyError, Result<T>
      proto.rs                      ← include_proto! re-export
  node/                             ← package: oxy-node
    Cargo.toml
    src/
      lib.rs                        ← pub async fn run(config: NodeConfig) → Result<()>
  panel/                            ← package: oxy-panel
    Cargo.toml
    src/
      lib.rs                        ← pub async fn run(config: PanelConfig) → Result<()>
  cli/                              ← package: oxydactylus (binary)
    Cargo.toml
    src/
      main.rs                       ← parse CLI args, read config, dispatch role
```

---

### Task 1: Workspace root

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `.gitignore`

**Interfaces:**
- Produces: `cargo metadata --no-deps` exits 0 and lists all 4 workspace members

- [ ] **Step 1: Create workspace Cargo.toml**

```toml
[workspace]
members  = ["crates/core", "crates/node", "crates/panel", "crates/cli"]
resolver = "2"

[workspace.dependencies]
tokio              = { version = "1",    features = ["full"] }
tonic              = { version = "0.12", features = ["transport"] }
prost              = "0.13"
serde              = { version = "1",    features = ["derive"] }
toml               = "0.8"
thiserror          = "2"
anyhow             = "1"
tracing            = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
uuid               = { version = "1",   features = ["v4", "serde"] }

[profile.release]
opt-level     = 3
lto           = "thin"
codegen-units = 1
strip         = true
```

- [ ] **Step 2: Create rust-toolchain.toml**

```toml
[toolchain]
channel = "stable"
```

- [ ] **Step 3: Create .gitignore**

```
/target
**/*.rs.bk
.env
config.toml
```

- [ ] **Step 4: Verify workspace resolves**

Run: `cargo metadata --no-deps --format-version 1 2>&1 | python3 -c "import sys,json; d=json.load(sys.stdin); print([p['name'] for p in d['packages']])"`

Expected: `['oxy-core', 'oxy-node', 'oxy-panel', 'oxydactylus']` (order may vary). If crates don't exist yet, `cargo metadata` will error — that's OK, it will pass after Task 2–5.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml rust-toolchain.toml .gitignore
git commit -m "chore: init cargo workspace"
```

---

### Task 2: oxy-core — config and error types

**Files:**
- Create: `crates/core/Cargo.toml`
- Create: `crates/core/src/lib.rs`
- Create: `crates/core/src/config.rs`
- Create: `crates/core/src/error.rs`

**Interfaces:**
- Produces:
  - `oxy_core::Config { role: RoleSection, panel: Option<PanelConfig>, node: Option<NodeConfig> }`
  - `oxy_core::RoleSection { kind: Role }`
  - `oxy_core::Role` — enum `Panel | Node | Both`
  - `oxy_core::PanelConfig { http_listen: String, database_url: String }`
  - `oxy_core::NodeConfig { grpc_listen: String, token: String }`
  - `oxy_core::OxyError` — enum error type
  - `oxy_core::Result<T>` — type alias for `std::result::Result<T, OxyError>`

- [ ] **Step 1: Create crates/core/Cargo.toml**

```toml
[package]
name    = "oxy-core"
version = "0.1.0"
edition = "2021"

[dependencies]
serde     = { workspace = true }
toml      = { workspace = true }
thiserror = { workspace = true }
tonic     = { workspace = true }
prost     = { workspace = true }

[build-dependencies]
tonic-build = "0.12"
```

- [ ] **Step 2: Write failing tests for config parsing**

Create `crates/core/src/config.rs`:

```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Panel,
    Node,
    Both,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RoleSection {
    #[serde(rename = "type")]
    pub kind: Role,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PanelConfig {
    pub http_listen:  String,
    pub database_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodeConfig {
    pub grpc_listen: String,
    pub token:       String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub role:  RoleSection,
    pub panel: Option<PanelConfig>,
    pub node:  Option<NodeConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_panel_role() {
        let raw = r#"
[role]
type = "panel"

[panel]
http_listen  = "0.0.0.0:3000"
database_url = "postgres://localhost/oxy"
"#;
        let cfg: Config = toml::from_str(raw).unwrap();
        assert_eq!(cfg.role.kind, Role::Panel);
        let panel = cfg.panel.unwrap();
        assert_eq!(panel.http_listen, "0.0.0.0:3000");
        assert_eq!(panel.database_url, "postgres://localhost/oxy");
        assert!(cfg.node.is_none());
    }

    #[test]
    fn parses_node_role() {
        let raw = r#"
[role]
type = "node"

[node]
grpc_listen = "0.0.0.0:8080"
token       = "secret-token"
"#;
        let cfg: Config = toml::from_str(raw).unwrap();
        assert_eq!(cfg.role.kind, Role::Node);
        let node = cfg.node.unwrap();
        assert_eq!(node.grpc_listen, "0.0.0.0:8080");
        assert_eq!(node.token, "secret-token");
        assert!(cfg.panel.is_none());
    }

    #[test]
    fn parses_both_role() {
        let raw = r#"
[role]
type = "both"

[panel]
http_listen  = "0.0.0.0:3000"
database_url = "postgres://localhost/oxy"

[node]
grpc_listen = "0.0.0.0:8080"
token       = "secret-token"
"#;
        let cfg: Config = toml::from_str(raw).unwrap();
        assert_eq!(cfg.role.kind, Role::Both);
        assert!(cfg.panel.is_some());
        assert!(cfg.node.is_some());
    }

    #[test]
    fn rejects_unknown_role() {
        let raw = r#"
[role]
type = "invalid"
"#;
        let result: Result<Config, _> = toml::from_str(raw);
        assert!(result.is_err());
    }
}
```

- [ ] **Step 3: Run tests to confirm they fail (module not found)**

Run: `cargo test -p oxy-core 2>&1 | tail -5`

Expected: compile error — `lib.rs` does not export `config` yet.

- [ ] **Step 4: Create crates/core/src/error.rs**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OxyError {
    #[error("config error: {0}")]
    Config(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("grpc error: {0}")]
    Grpc(#[from] tonic::Status),
}

pub type Result<T> = std::result::Result<T, OxyError>;
```

- [ ] **Step 5: Create crates/core/src/lib.rs**

```rust
pub mod config;
pub mod error;

pub use config::{Config, NodeConfig, PanelConfig, Role, RoleSection};
pub use error::{OxyError, Result};
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p oxy-core`

Expected:
```
test config::tests::parses_panel_role  ... ok
test config::tests::parses_node_role   ... ok
test config::tests::parses_both_role   ... ok
test config::tests::rejects_unknown_role ... ok
test result: ok. 4 passed; 0 failed
```

- [ ] **Step 7: Commit**

```bash
git add crates/core/
git commit -m "feat(oxy-core): config and error types with tests"
```

---

### Task 3: oxy-core — proto definitions

**Files:**
- Create: `crates/core/proto/oxydactylus.proto`
- Create: `crates/core/build.rs`
- Create: `crates/core/src/proto.rs`
- Modify: `crates/core/src/lib.rs` — add `pub mod proto`

**Interfaces:**
- Consumes: Task 2 (crate structure exists)
- Produces (all under `oxy_core::proto::node::`):
  - `NodeServiceClient<T>` — gRPC client (used by panel in Plan 3)
  - `NodeServiceServer<T>` — gRPC server (used by node in Plan 2)
  - `ServerStartRequest { server_id: String }`
  - `ServerStopRequest { server_id: String, timeout: u32 }`
  - `ServerDeleteRequest { server_id: String }`
  - `ServerCommandRequest { server_id: String, content: String }`
  - `ServerLogsRequest { server_id: String, follow: bool }`
  - `ServerStatsRequest { server_id: String }`
  - `ServerReply { success: bool, message: String }`
  - `ServerStats { server_id: String, memory_bytes: u64, cpu_percent: f64, rx_bytes: u64, tx_bytes: u64 }`
  - `LogLine { content: String, stream: String, timestamp: i64 }`

- [ ] **Step 1: Create proto file**

Create `crates/core/proto/oxydactylus.proto`:

```protobuf
syntax = "proto3";

package oxydactylus.node;

service NodeService {
    rpc StartServer  (ServerStartRequest)   returns (ServerReply);
    rpc StopServer   (ServerStopRequest)    returns (ServerReply);
    rpc DeleteServer (ServerDeleteRequest)  returns (ServerReply);
    rpc GetStats     (ServerStatsRequest)   returns (ServerStats);
    rpc StreamLogs   (ServerLogsRequest)    returns (stream LogLine);
    rpc SendCommand  (ServerCommandRequest) returns (ServerReply);
}

message ServerStartRequest  { string server_id = 1; }
message ServerDeleteRequest { string server_id = 1; }
message ServerStatsRequest  { string server_id = 1; }

message ServerStopRequest {
    string server_id = 1;
    uint32 timeout   = 2;
}

message ServerCommandRequest {
    string server_id = 1;
    string content   = 2;
}

message ServerLogsRequest {
    string server_id = 1;
    bool   follow    = 2;
}

message ServerReply {
    bool   success = 1;
    string message = 2;
}

message ServerStats {
    string server_id    = 1;
    uint64 memory_bytes = 2;
    double cpu_percent  = 3;
    uint64 rx_bytes     = 4;
    uint64 tx_bytes     = 5;
}

message LogLine {
    string content   = 1;
    string stream    = 2;
    int64  timestamp = 3;
}
```

- [ ] **Step 2: Create build.rs**

Create `crates/core/build.rs`:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile(
            &["proto/oxydactylus.proto"],
            &["proto"],
        )?;
    Ok(())
}
```

- [ ] **Step 3: Create proto.rs**

Create `crates/core/src/proto.rs`:

```rust
pub mod node {
    tonic::include_proto!("oxydactylus.node");
}
```

- [ ] **Step 4: Add proto module to lib.rs**

Edit `crates/core/src/lib.rs` — add `pub mod proto;` after the existing modules:

```rust
pub mod config;
pub mod error;
pub mod proto;

pub use config::{Config, NodeConfig, PanelConfig, Role, RoleSection};
pub use error::{OxyError, Result};
```

- [ ] **Step 5: Verify proto compiles**

Run: `cargo build -p oxy-core`

Expected: `Finished dev [unoptimized + debuginfo]`

If `protoc` is missing:
```bash
# Ubuntu/Debian
apt-get install -y protobuf-compiler
# macOS
brew install protobuf
```
Then re-run `cargo build -p oxy-core`.

- [ ] **Step 6: Verify generated types are accessible**

Run:
```bash
cargo doc -p oxy-core --no-deps 2>&1 | tail -3
```
Expected: `Finished` with no errors. Types under `oxy_core::proto::node` appear in docs.

- [ ] **Step 7: Commit**

```bash
git add crates/core/
git commit -m "feat(oxy-core): grpc proto definitions (NodeService)"
```

---

### Task 4: oxy-node stub

**Files:**
- Create: `crates/node/Cargo.toml`
- Create: `crates/node/src/lib.rs`

**Interfaces:**
- Consumes: `oxy_core::NodeConfig`, `oxy_core::Result`
- Produces: `oxy_node::run(config: NodeConfig) -> oxy_core::Result<()>` — logs listen address, pends forever

- [ ] **Step 1: Create crates/node/Cargo.toml**

```toml
[package]
name    = "oxy-node"
version = "0.1.0"
edition = "2021"

[dependencies]
oxy-core  = { path = "../core" }
tokio     = { workspace = true }
tonic     = { workspace = true }
tracing   = { workspace = true }
thiserror = { workspace = true }
```

- [ ] **Step 2: Create crates/node/src/lib.rs**

```rust
use oxy_core::{NodeConfig, Result};

pub async fn run(config: NodeConfig) -> Result<()> {
    tracing::info!(listen = %config.grpc_listen, "node starting");
    std::future::pending::<()>().await;
    Ok(())
}
```

- [ ] **Step 3: Verify node compiles**

Run: `cargo build -p oxy-node`

Expected: `Finished dev` with no errors or warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/node/
git commit -m "feat(oxy-node): stub crate"
```

---

### Task 5: oxy-panel stub

**Files:**
- Create: `crates/panel/Cargo.toml`
- Create: `crates/panel/src/lib.rs`

**Interfaces:**
- Consumes: `oxy_core::PanelConfig`, `oxy_core::Result`
- Produces: `oxy_panel::run(config: PanelConfig) -> oxy_core::Result<()>` — logs listen address, pends forever

- [ ] **Step 1: Create crates/panel/Cargo.toml**

```toml
[package]
name    = "oxy-panel"
version = "0.1.0"
edition = "2021"

[dependencies]
oxy-core = { path = "../core" }
tokio    = { workspace = true }
tracing  = { workspace = true }
```

- [ ] **Step 2: Create crates/panel/src/lib.rs**

```rust
use oxy_core::{PanelConfig, Result};

pub async fn run(config: PanelConfig) -> Result<()> {
    tracing::info!(listen = %config.http_listen, "panel starting");
    std::future::pending::<()>().await;
    Ok(())
}
```

- [ ] **Step 3: Verify panel compiles**

Run: `cargo build -p oxy-panel`

Expected: `Finished dev` with no errors or warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/panel/
git commit -m "feat(oxy-panel): stub crate"
```

---

### Task 6: CLI entrypoint + integration smoke test

**Files:**
- Create: `crates/cli/Cargo.toml`
- Create: `crates/cli/src/main.rs`
- Create: `config.example.toml`

**Interfaces:**
- Consumes: `oxy_core::Config`, `oxy_core::Role`, `oxy_node::run`, `oxy_panel::run`
- Produces: `oxydactylus` binary — parses `--config <path>`, dispatches role, propagates errors with clear messages

- [ ] **Step 1: Create crates/cli/Cargo.toml**

```toml
[package]
name    = "oxydactylus"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "oxydactylus"
path = "src/main.rs"

[dependencies]
oxy-core   = { path = "../core" }
oxy-node   = { path = "../node" }
oxy-panel  = { path = "../panel" }
tokio      = { workspace = true }
clap       = { version = "4", features = ["derive"] }
anyhow     = { workspace = true }
tracing    = { workspace = true }
tracing-subscriber = { workspace = true }
toml       = { workspace = true }
```

- [ ] **Step 2: Create crates/cli/src/main.rs**

```rust
use std::path::PathBuf;
use clap::Parser;
use oxy_core::Role;

#[derive(Parser)]
#[command(name = "oxydactylus", version, about = "Game server management panel")]
struct Cli {
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let raw = std::fs::read_to_string(&cli.config)
        .map_err(|e| anyhow::anyhow!("cannot read {:?}: {}", cli.config, e))?;

    let config: oxy_core::Config = toml::from_str(&raw)
        .map_err(|e| anyhow::anyhow!("invalid config.toml: {}", e))?;

    match config.role.kind {
        Role::Panel => {
            let cfg = config.panel
                .ok_or_else(|| anyhow::anyhow!("[panel] section required when role = \"panel\""))?;
            oxy_panel::run(cfg).await?;
        }
        Role::Node => {
            let cfg = config.node
                .ok_or_else(|| anyhow::anyhow!("[node] section required when role = \"node\""))?;
            oxy_node::run(cfg).await?;
        }
        Role::Both => {
            let panel_cfg = config.panel
                .ok_or_else(|| anyhow::anyhow!("[panel] section required when role = \"both\""))?;
            let node_cfg = config.node
                .ok_or_else(|| anyhow::anyhow!("[node] section required when role = \"both\""))?;
            let (p, n) = tokio::join!(oxy_panel::run(panel_cfg), oxy_node::run(node_cfg));
            p?;
            n?;
        }
    }

    Ok(())
}
```

- [ ] **Step 3: Create config.example.toml**

```toml
# config.example.toml
# Copy to config.toml and edit before running.

[role]
type = "both"

[panel]
http_listen  = "0.0.0.0:3000"
database_url = "postgres://oxy:oxy@localhost/oxy"

[node]
grpc_listen = "0.0.0.0:8080"
token       = "change-me-to-a-random-secret"
```

- [ ] **Step 4: Build the full workspace**

Run: `cargo build`

Expected:
```
Finished dev [unoptimized + debuginfo] target(s) in Xs
```
Binary at `target/debug/oxydactylus`.

- [ ] **Step 5: Verify --help works**

Run: `./target/debug/oxydactylus --help`

Expected:
```
Game server management panel

Usage: oxydactylus [OPTIONS]

Options:
  -c, --config <CONFIG>  [default: config.toml]
  -h, --help             Print help
  -V, --version          Print version
```

- [ ] **Step 6: Verify missing config gives clear error**

Run: `./target/debug/oxydactylus --config /nonexistent 2>&1`

Expected:
```
Error: cannot read "/nonexistent": No such file or directory (os error 2)
```

- [ ] **Step 7: Verify missing section gives clear error**

```bash
echo '[role]
type = "panel"' > /tmp/bad.toml
./target/debug/oxydactylus --config /tmp/bad.toml 2>&1
```

Expected:
```
Error: [panel] section required when role = "panel"
```

- [ ] **Step 8: Verify role dispatch logs correct addresses**

```bash
cp config.example.toml config.toml
RUST_LOG=info timeout 2 ./target/debug/oxydactylus || true
```

Expected log output (order may vary):
```
INFO oxy_panel: panel starting listen=0.0.0.0:3000
INFO oxy_node: node starting listen=0.0.0.0:8080
```

- [ ] **Step 9: Commit**

```bash
git add crates/cli/ config.example.toml
git commit -m "feat(cli): entrypoint with role dispatch"
git push origin main
```

---

## Self-Review

**Spec coverage check:**

| Spec requirement | Covered by |
|---|---|
| Binário único, roles panel/node/both | Task 6 (CLI dispatch) |
| `tokio::join!` para role=both | Task 6 Step 2 |
| Config via `config.toml` com `[role].type` | Task 2 + Task 6 |
| `[panel].http_listen`, `database_url` | Task 2 (`PanelConfig`) |
| `[node].grpc_listen`, `token` | Task 2 (`NodeConfig`) |
| Proto `NodeService` com 6 RPCs | Task 3 |
| `oxy_core::proto::node::*` como único ponto de import | Task 3 (`proto.rs`) |
| `tonic-build` via `build.rs` em `core` | Task 3 |
| Mensagens proto com tipos corretos | Task 3 (`.proto`) |
| Erros tipados com `thiserror` | Task 2 (`error.rs`) |
| Tracing em vez de println | Task 4, 5, 6 |

**Não coberto neste plano (intencionalmente — próximos planos):**
- gRPC server real (Plan 2 — Node daemon)
- Bollard / Docker (Plan 2)
- Axum / Leptos / banco de dados (Plans 3–5)
- Eggs / importer (Plan 4)
- Build musl / CI (Plan 6)

**Placeholder scan:** Nenhum TBD, TODO ou step sem código encontrado.

**Type consistency:** `NodeConfig` e `PanelConfig` definidos em Task 2 são consumidos em Tasks 4, 5 e 6 com os mesmos nomes e campos. `oxy_core::Result<()>` retornado por `run()` em ambos os stubs é propagado corretamente via `?` no CLI.
