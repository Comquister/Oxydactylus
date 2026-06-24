# Oxydactylus Node Daemon — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transformar o crate `oxy-node` do stub atual em um daemon gRPC funcional que gerencia containers Docker via bollard, autentica chamadas do panel via tonic Interceptor e faz streaming de logs em tempo real.

**Architecture:** O crate expõe um `DockerBackend` trait (mockável via `mockall`) que abstrai todas as chamadas bollard. O `NodeServiceImpl<B: DockerBackend>` implementa os 6 RPCs do proto usando essa trait. A autenticação é feita por um `AuthInterceptor` tonic que valida o header `authorization: Bearer <token>` contra `NodeConfig.token`. O `forward_logs` helper em `stream.rs` drena o stream do Docker para um canal tokio e para imediatamente se o receiver fechar.

**Tech Stack:** bollard 0.17, mockall 0.13, async-trait 0.1, futures-util 0.3, tokio-stream 0.1, tonic 0.12 (já no workspace), thiserror 2 (já no workspace)

## Global Constraints

- Rust edition: 2021
- `DockerBackend` trait usa `String` (não `&str`) em todos os parâmetros — compatibilidade com mockall
- Código de negócio nunca chama `bollard` diretamente — apenas via `DockerBackend` trait
- `SendCommand` usa stdin attach do PID 1 — nunca `docker exec`
- Containers criados com `open_stdin: true`, `stdin_once: false`, limites de memória e CPU obrigatórios
- `StreamLogs`: break imediato no `tx.send().await.is_err()` — sem task fantasma
- Sem `unwrap()` em código de produção; sem `println!` — usar `tracing`
- Node-local `NodeError` enum (não polui `OxyError` com erros Docker)
- `StartServer` inicia um container existente pelo nome (`server_id` = nome do container); criação de container (`create_container`) é chamada pelo panel na instalação (Plans 3/4)

---

## File Map

```
crates/node/
├── Cargo.toml                  ← add bollard, mockall, async-trait, futures-util, tokio-stream
└── src/
    ├── lib.rs                  ← substitui stub: inicia servidor tonic real
    ├── error.rs                ← NodeError enum + From<bollard::errors::Error> + Into<Status>
    ├── docker.rs               ← DockerBackend trait + ContainerSpec/Stats/LogChunk types + BollardDocker impl
    ├── stream.rs               ← forward_logs(stream, tx) helper
    ├── interceptor.rs          ← AuthInterceptor (tonic::service::Interceptor)
    └── server.rs               ← NodeServiceImpl<B: DockerBackend>: implementa os 6 RPCs

crates/core/Cargo.toml         ← adicionar async-trait, futures-util, tokio-stream ao workspace
Cargo.toml (workspace root)    ← adicionar async-trait, futures-util, tokio-stream ao [workspace.dependencies]
```

---

### Task 1: Dependências, tipos e DockerBackend trait

**Files:**
- Modify: `Cargo.toml` (workspace root) — adicionar `async-trait`, `futures-util`, `tokio-stream`
- Modify: `crates/node/Cargo.toml` — adicionar todas as novas deps
- Create: `crates/node/src/error.rs`
- Create: `crates/node/src/docker.rs` — trait + tipos (sem implementação bollard ainda)
- Modify: `crates/node/src/lib.rs` — adicionar declarações de módulo

**Interfaces:**
- Produces:
  - `node::error::NodeError` — enum com variantes `Docker(String)`, `NotFound(String)`, `Validation(String)`
  - `node::error::Result<T>` — alias para `std::result::Result<T, NodeError>`
  - `node::docker::ContainerSpec { image: String, name: String, env: Vec<String>, memory_mb: i64, cpu_percent: i64 }`
  - `node::docker::ContainerStats { memory_bytes: u64, cpu_percent: f64, rx_bytes: u64, tx_bytes: u64 }`
  - `node::docker::LogChunk { content: String, stream: String }`
  - `node::docker::DockerBackend` — trait com 7 métodos async
  - `node::docker::MockDockerBackend` — gerado pelo mockall em `#[cfg(test)]`

- [ ] **Step 1: Adicionar deps ao workspace Cargo.toml**

Editar `/opt/Oxydactylus/Cargo.toml`, na seção `[workspace.dependencies]`, adicionar após `uuid`:

```toml
async-trait  = "0.1"
futures-util = "0.3"
tokio-stream = "0.1"
```

- [ ] **Step 2: Atualizar crates/node/Cargo.toml**

```toml
[package]
name    = "oxy-node"
version = "0.1.0"
edition = "2021"

[dependencies]
oxy-core     = { path = "../core" }
tokio        = { workspace = true }
tonic        = { workspace = true }
tracing      = { workspace = true }
thiserror    = { workspace = true }
async-trait  = { workspace = true }
futures-util = { workspace = true }
tokio-stream = { workspace = true }
bollard      = "0.17"

[dev-dependencies]
mockall    = "0.13"
tokio-test = "0.4"
```

- [ ] **Step 3: Criar crates/node/src/error.rs**

```rust
use tonic::Status;

#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    #[error("docker error: {0}")]
    Docker(String),
    #[error("container not found: {0}")]
    NotFound(String),
    #[error("validation error: {0}")]
    Validation(String),
}

pub type Result<T> = std::result::Result<T, NodeError>;

impl From<bollard::errors::Error> for NodeError {
    fn from(e: bollard::errors::Error) -> Self {
        use bollard::errors::Error as BE;
        match &e {
            BE::DockerResponseServerError { status_code: 404, .. } => {
                NodeError::NotFound(e.to_string())
            }
            _ => NodeError::Docker(e.to_string()),
        }
    }
}

impl From<NodeError> for Status {
    fn from(e: NodeError) -> Self {
        match e {
            NodeError::Docker(msg)     => Status::internal(msg),
            NodeError::NotFound(msg)   => Status::not_found(msg),
            NodeError::Validation(msg) => Status::invalid_argument(msg),
        }
    }
}
```

- [ ] **Step 4: Criar crates/node/src/docker.rs com trait e tipos**

```rust
use async_trait::async_trait;
use futures_util::stream::BoxStream;
use crate::error::{NodeError, Result};

pub struct ContainerSpec {
    pub image:       String,
    pub name:        String,
    pub env:         Vec<String>,
    pub memory_mb:   i64,
    pub cpu_percent: i64,
}

pub struct ContainerStats {
    pub memory_bytes: u64,
    pub cpu_percent:  f64,
    pub rx_bytes:     u64,
    pub tx_bytes:     u64,
}

pub struct LogChunk {
    pub content: String,
    pub stream:  String,
}

#[async_trait]
#[cfg_attr(test, mockall::automock)]
pub trait DockerBackend: Send + Sync + 'static {
    async fn create_container(&self, spec: ContainerSpec) -> Result<String>;
    async fn start_container(&self, id: String)  -> Result<()>;
    async fn stop_container(&self, id: String, timeout: u32) -> Result<()>;
    async fn delete_container(&self, id: String) -> Result<()>;
    async fn send_command(&self, id: String, command: String) -> Result<()>;
    async fn get_stats(&self, id: String) -> Result<ContainerStats>;
    async fn log_stream(&self, id: String, follow: bool)
        -> Result<BoxStream<'static, Result<LogChunk>>>;
}

pub struct BollardDocker {
    inner: bollard::Docker,
}

impl BollardDocker {
    pub fn connect() -> Result<Self> {
        let inner = bollard::Docker::connect_with_local_defaults()
            .map_err(|e| NodeError::Docker(e.to_string()))?;
        Ok(Self { inner })
    }
}
```

- [ ] **Step 5: Atualizar crates/node/src/lib.rs com declarações de módulo**

```rust
pub mod docker;
pub mod error;
pub mod interceptor;
pub mod server;
pub mod stream;

use oxy_core::{NodeConfig, OxyError};

pub async fn run(config: NodeConfig) -> oxy_core::Result<()> {
    tracing::info!(listen = %config.grpc_listen, "node starting");
    std::future::pending::<()>().await;
    Ok(())
}
```

(O `run()` real será implementado na Task 8; por agora mantém o placeholder para compilar.)

- [ ] **Step 6: Criar arquivos vazios para os outros módulos**

```bash
touch crates/node/src/interceptor.rs
touch crates/node/src/server.rs
touch crates/node/src/stream.rs
```

Adicionar conteúdo mínimo em cada um para compilar:

`crates/node/src/interceptor.rs`:
```rust
// implementado na Task 6
```

`crates/node/src/server.rs`:
```rust
// implementado na Task 7
```

`crates/node/src/stream.rs`:
```rust
// implementado na Task 4
```

- [ ] **Step 7: Verificar que compila**

Run: `cargo build -p oxy-node 2>&1 | tail -5`

Expected: `Finished dev` — sem erros de compilação.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml crates/node/
git commit -m "feat(oxy-node): add deps, error types, and DockerBackend trait"
```

---

### Task 2: BollardDocker — ciclo de vida de containers

**Files:**
- Modify: `crates/node/src/docker.rs` — implementar `create_container`, `start_container`, `stop_container`, `delete_container` em `BollardDocker`

**Interfaces:**
- Consumes: `ContainerSpec`, `NodeError`, `BollardDocker` da Task 1
- Produces: implementação real dos 4 métodos + `MockDockerBackend` disponível em testes

- [ ] **Step 1: Escrever testes com MockDockerBackend**

Adicionar ao final de `crates/node/src/docker.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_create_container_returns_id() {
        let mut mock = MockDockerBackend::new();
        mock.expect_create_container()
            .once()
            .returning(|spec| {
                assert_eq!(spec.image, "nginx:latest");
                assert_eq!(spec.name, "srv-1");
                assert_eq!(spec.memory_mb, 512);
                assert_eq!(spec.cpu_percent, 50);
                Ok("abc123".to_string())
            });

        let id = mock.create_container(ContainerSpec {
            image:       "nginx:latest".into(),
            name:        "srv-1".into(),
            env:         vec!["PORT=25565".into()],
            memory_mb:   512,
            cpu_percent: 50,
        }).await.unwrap();

        assert_eq!(id, "abc123");
    }

    #[tokio::test]
    async fn mock_start_container_called_with_id() {
        let mut mock = MockDockerBackend::new();
        mock.expect_start_container()
            .withf(|id| id == "abc123")
            .once()
            .returning(|_| Ok(()));

        mock.start_container("abc123".into()).await.unwrap();
    }

    #[tokio::test]
    async fn mock_stop_container_passes_timeout() {
        let mut mock = MockDockerBackend::new();
        mock.expect_stop_container()
            .withf(|id, timeout| id == "abc123" && *timeout == 10)
            .once()
            .returning(|_, _| Ok(()));

        mock.stop_container("abc123".into(), 10).await.unwrap();
    }

    #[tokio::test]
    async fn mock_delete_container_called_with_id() {
        let mut mock = MockDockerBackend::new();
        mock.expect_delete_container()
            .withf(|id| id == "abc123")
            .once()
            .returning(|_| Ok(()));

        mock.delete_container("abc123".into()).await.unwrap();
    }
}
```

- [ ] **Step 2: Rodar testes para confirmar que passam (mock não precisa de impl real)**

Run: `cargo test -p oxy-node docker::tests 2>&1 | tail -10`

Expected:
```
test docker::tests::mock_create_container_returns_id ... ok
test docker::tests::mock_start_container_called_with_id ... ok
test docker::tests::mock_stop_container_passes_timeout ... ok
test docker::tests::mock_delete_container_called_with_id ... ok
test result: ok. 4 passed; 0 failed
```

- [ ] **Step 3: Implementar os 4 métodos em BollardDocker**

Adicionar ao `docker.rs` após a definição de `BollardDocker`:

```rust
#[async_trait]
impl DockerBackend for BollardDocker {
    async fn create_container(&self, spec: ContainerSpec) -> Result<String> {
        use bollard::container::{Config, CreateContainerOptions};
        use bollard::models::HostConfig;

        if spec.memory_mb <= 0 || spec.cpu_percent <= 0 {
            return Err(NodeError::Validation(
                "memory_mb and cpu_percent must be positive".into(),
            ));
        }

        let opts = CreateContainerOptions {
            name:     spec.name.as_str(),
            platform: None,
        };
        let cfg = Config {
            image:       Some(spec.image.as_str()),
            env:         Some(spec.env.iter().map(String::as_str).collect()),
            open_stdin:  Some(true),
            stdin_once:  Some(false),
            host_config: Some(HostConfig {
                memory:    Some(spec.memory_mb * 1024 * 1024),
                nano_cpus: Some(spec.cpu_percent * 10_000_000),
                ..Default::default()
            }),
            ..Default::default()
        };
        let resp = self.inner.create_container(Some(opts), cfg).await?;
        Ok(resp.id)
    }

    async fn start_container(&self, id: String) -> Result<()> {
        self.inner
            .start_container::<String>(&id, None)
            .await
            .map_err(NodeError::from)
    }

    async fn stop_container(&self, id: String, timeout: u32) -> Result<()> {
        use bollard::container::StopContainerOptions;
        self.inner
            .stop_container(&id, Some(StopContainerOptions { t: timeout as i64 }))
            .await
            .map_err(NodeError::from)
    }

    async fn delete_container(&self, id: String) -> Result<()> {
        use bollard::container::RemoveContainerOptions;
        self.inner
            .remove_container(&id, Some(RemoveContainerOptions {
                v:     true,
                force: false,
                link:  false,
            }))
            .await
            .map_err(NodeError::from)
    }

    async fn send_command(&self, _id: String, _command: String) -> Result<()> {
        unimplemented!("implemented in Task 3")
    }

    async fn get_stats(&self, _id: String) -> Result<ContainerStats> {
        unimplemented!("implemented in Task 5")
    }

    async fn log_stream(&self, _id: String, _follow: bool)
        -> Result<BoxStream<'static, Result<LogChunk>>>
    {
        unimplemented!("implemented in Task 4")
    }
}
```

- [ ] **Step 4: Verificar que compila**

Run: `cargo build -p oxy-node 2>&1 | tail -5`

Expected: `Finished dev` — os `unimplemented!` não causam erro de compilação.

- [ ] **Step 5: Rodar todos os testes**

Run: `cargo test -p oxy-node 2>&1 | tail -10`

Expected: 4 testes passando; os `unimplemented!` não são exercitados pelos testes mock.

- [ ] **Step 6: Commit**

```bash
git add crates/node/src/docker.rs
git commit -m "feat(oxy-node): bollard container lifecycle (create/start/stop/delete)"
```

---

### Task 3: BollardDocker — SendCommand via stdin attach

**Files:**
- Modify: `crates/node/src/docker.rs` — implementar `send_command` em `BollardDocker`

**Interfaces:**
- Consumes: `BollardDocker` da Task 2; `NodeError`
- Produces: `send_command(id, command)` que escreve `"command\n"` no stdin do PID 1 do container

- [ ] **Step 1: Adicionar teste mock para send_command**

Adicionar dentro de `#[cfg(test)] mod tests` em `docker.rs`:

```rust
    #[tokio::test]
    async fn mock_send_command_receives_command_with_newline() {
        let mut mock = MockDockerBackend::new();
        mock.expect_send_command()
            .withf(|id, cmd| id == "srv-1" && cmd == "say hello\n")
            .once()
            .returning(|_, _| Ok(()));

        mock.send_command("srv-1".into(), "say hello\n".into()).await.unwrap();
    }
```

Run: `cargo test -p oxy-node docker::tests::mock_send_command 2>&1 | tail -5`

Expected: `test docker::tests::mock_send_command_receives_command_with_newline ... ok`

- [ ] **Step 2: Implementar send_command em BollardDocker**

Substituir o `unimplemented!` de `send_command` na impl de `BollardDocker`:

```rust
    async fn send_command(&self, id: String, command: String) -> Result<()> {
        use bollard::container::AttachContainerOptions;
        use tokio::io::AsyncWriteExt;

        let mut attach = self.inner
            .attach_container(&id, Some(AttachContainerOptions::<String> {
                stdin:  Some(true),
                stream: Some(true),
                stdout: Some(false),
                stderr: Some(false),
                ..Default::default()
            }))
            .await
            .map_err(NodeError::from)?;

        let payload = format!("{}\n", command);
        attach.input
            .write_all(payload.as_bytes())
            .await
            .map_err(|e| NodeError::Docker(e.to_string()))
    }
```

- [ ] **Step 3: Verificar que compila**

Run: `cargo build -p oxy-node 2>&1 | tail -5`

Expected: `Finished dev`

- [ ] **Step 4: Commit**

```bash
git add crates/node/src/docker.rs
git commit -m "feat(oxy-node): send_command via stdin attach (never docker exec)"
```

---

### Task 4: BollardDocker — log_stream + stream.rs forward_logs

**Files:**
- Modify: `crates/node/src/docker.rs` — implementar `log_stream` em `BollardDocker`
- Modify: `crates/node/src/stream.rs` — implementar `forward_logs` helper

**Interfaces:**
- Consumes: `LogChunk`, `NodeError`, `DockerBackend`
- Produces:
  - `BollardDocker::log_stream(id, follow) -> Result<BoxStream<'static, Result<LogChunk>>>`
  - `stream::forward_logs(stream, tx)` — drena stream de LogChunk para `mpsc::Sender<Result<LogLine, Status>>`, para no `is_err()`

- [ ] **Step 1: Escrever testes para forward_logs**

Substituir o conteúdo de `crates/node/src/stream.rs`:

```rust
use futures_util::{stream::BoxStream, StreamExt};
use tokio::sync::mpsc;
use tonic::Status;
use oxy_core::proto::node::LogLine;
use crate::docker::LogChunk;
use crate::error::Result as NodeResult;

pub async fn forward_logs(
    mut stream: BoxStream<'static, NodeResult<LogChunk>>,
    tx: mpsc::Sender<Result<LogLine, Status>>,
) {
    while let Some(chunk) = stream.next().await {
        let line = match chunk {
            Ok(c) => LogLine {
                content:   c.content,
                stream:    c.stream,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
            },
            Err(e) => {
                let _ = tx.send(Err(Status::internal(e.to_string()))).await;
                return;
            }
        };
        if tx.send(Ok(line)).await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::NodeError;
    use futures_util::stream;

    #[tokio::test]
    async fn forward_logs_sends_all_chunks() {
        let chunks: Vec<NodeResult<LogChunk>> = vec![
            Ok(LogChunk { content: "line1\n".into(), stream: "stdout".into() }),
            Ok(LogChunk { content: "line2\n".into(), stream: "stderr".into() }),
        ];
        let s = Box::pin(stream::iter(chunks));
        let (tx, mut rx) = mpsc::channel(10);

        forward_logs(s, tx).await;

        let msg1 = rx.recv().await.unwrap().unwrap();
        assert_eq!(msg1.content, "line1\n");
        assert_eq!(msg1.stream,  "stdout");

        let msg2 = rx.recv().await.unwrap().unwrap();
        assert_eq!(msg2.content, "line2\n");
        assert_eq!(msg2.stream,  "stderr");

        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn forward_logs_stops_when_receiver_dropped() {
        // Stream com muitos itens; receiver dropado imediatamente
        let chunks: Vec<NodeResult<LogChunk>> = (0..100)
            .map(|i| Ok(LogChunk { content: format!("line{}\n", i), stream: "stdout".into() }))
            .collect();
        let s = Box::pin(stream::iter(chunks));
        let (tx, rx) = mpsc::channel(1);
        drop(rx); // receiver dropado: send vai falhar imediatamente

        // Deve retornar sem bloquear nem vazar
        forward_logs(s, tx).await;
    }

    #[tokio::test]
    async fn forward_logs_sends_error_on_stream_error() {
        let chunks: Vec<NodeResult<LogChunk>> = vec![
            Err(NodeError::Docker("boom".into())),
        ];
        let s = Box::pin(stream::iter(chunks));
        let (tx, mut rx) = mpsc::channel(10);

        forward_logs(s, tx).await;

        let result = rx.recv().await.unwrap();
        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("boom"));
    }
}
```

- [ ] **Step 2: Rodar testes para confirmar que passam**

Run: `cargo test -p oxy-node stream::tests 2>&1 | tail -10`

Expected:
```
test stream::tests::forward_logs_sends_all_chunks ... ok
test stream::tests::forward_logs_stops_when_receiver_dropped ... ok
test stream::tests::forward_logs_sends_error_on_stream_error ... ok
test result: ok. 3 passed; 0 failed
```

- [ ] **Step 3: Implementar log_stream em BollardDocker**

Substituir o `unimplemented!` de `log_stream` na impl de `BollardDocker`:

```rust
    async fn log_stream(&self, id: String, follow: bool)
        -> Result<BoxStream<'static, Result<LogChunk>>>
    {
        use bollard::container::{LogOutput, LogsOptions};
        use futures_util::StreamExt;

        let opts = LogsOptions::<String> {
            stdout: true,
            stderr: true,
            follow,
            ..Default::default()
        };

        let stream = self.inner
            .logs(&id, Some(opts))
            .map(|result| {
                result.map_err(NodeError::from).map(|output| {
                    let stream_name = match &output {
                        LogOutput::StdOut { .. } => "stdout",
                        LogOutput::StdErr { .. } => "stderr",
                        _                        => "stdout",
                    }
                    .to_string();
                    LogChunk {
                        content: output.to_string(),
                        stream:  stream_name,
                    }
                })
            });

        Ok(Box::pin(stream))
    }
```

- [ ] **Step 4: Verificar que compila**

Run: `cargo build -p oxy-node 2>&1 | tail -5`

Expected: `Finished dev`

- [ ] **Step 5: Rodar todos os testes**

Run: `cargo test -p oxy-node 2>&1 | tail -10`

Expected: 7 testes passando (4 mock lifecycle + 3 stream).

- [ ] **Step 6: Commit**

```bash
git add crates/node/src/docker.rs crates/node/src/stream.rs
git commit -m "feat(oxy-node): log_stream + forward_logs helper with break-on-close"
```

---

### Task 5: BollardDocker — get_stats

**Files:**
- Modify: `crates/node/src/docker.rs` — implementar `get_stats` + adicionar teste mock

**Interfaces:**
- Consumes: `ContainerStats`, `NodeError`, `BollardDocker`
- Produces: `get_stats(id) -> Result<ContainerStats>` com memória, CPU%, rx/tx bytes

- [ ] **Step 1: Adicionar teste mock**

Dentro de `#[cfg(test)] mod tests` em `docker.rs`:

```rust
    #[tokio::test]
    async fn mock_get_stats_returns_container_stats() {
        let mut mock = MockDockerBackend::new();
        mock.expect_get_stats()
            .withf(|id| id == "srv-1")
            .once()
            .returning(|_| Ok(ContainerStats {
                memory_bytes: 256 * 1024 * 1024,
                cpu_percent:  12.5,
                rx_bytes:     1024,
                tx_bytes:     2048,
            }));

        let stats = mock.get_stats("srv-1".into()).await.unwrap();
        assert_eq!(stats.memory_bytes, 256 * 1024 * 1024);
        assert!((stats.cpu_percent - 12.5).abs() < 0.001);
        assert_eq!(stats.rx_bytes, 1024);
        assert_eq!(stats.tx_bytes, 2048);
    }
```

Run: `cargo test -p oxy-node docker::tests::mock_get_stats 2>&1 | tail -5`

Expected: `test docker::tests::mock_get_stats_returns_container_stats ... ok`

- [ ] **Step 2: Implementar get_stats em BollardDocker**

Substituir o `unimplemented!` de `get_stats`:

```rust
    async fn get_stats(&self, id: String) -> Result<ContainerStats> {
        use bollard::container::StatsOptions;
        use futures_util::StreamExt;

        let mut stream = self.inner.stats(&id, Some(StatsOptions {
            stream:   false,
            one_shot: Some(true),
        }));

        let stats = stream
            .next()
            .await
            .ok_or_else(|| NodeError::Docker("no stats returned".into()))??;

        let cpu_delta = stats.cpu_stats.cpu_usage.total_usage
            .saturating_sub(stats.precpu_stats.cpu_usage.total_usage);
        let system_delta = stats.cpu_stats.system_cpu_usage.unwrap_or(0)
            .saturating_sub(stats.precpu_stats.system_cpu_usage.unwrap_or(0));
        let num_cpus = stats.cpu_stats.online_cpus.unwrap_or(1) as f64;
        let cpu_percent = if system_delta > 0 {
            (cpu_delta as f64 / system_delta as f64) * num_cpus * 100.0
        } else {
            0.0
        };

        let memory_bytes = stats.memory_stats.usage.unwrap_or(0);

        let (rx_bytes, tx_bytes) = stats
            .networks
            .as_ref()
            .map(|nets| {
                nets.values().fold((0u64, 0u64), |(rx, tx), net| {
                    (rx + net.rx_bytes as u64, tx + net.tx_bytes as u64)
                })
            })
            .unwrap_or((0, 0));

        Ok(ContainerStats { memory_bytes, cpu_percent, rx_bytes, tx_bytes })
    }
```

- [ ] **Step 3: Verificar que compila e todos os testes passam**

Run: `cargo test -p oxy-node 2>&1 | tail -10`

Expected: 8 testes passando.

- [ ] **Step 4: Commit**

```bash
git add crates/node/src/docker.rs
git commit -m "feat(oxy-node): get_stats with cpu/memory/network metrics"
```

---

### Task 6: Auth Interceptor

**Files:**
- Modify: `crates/node/src/interceptor.rs`

**Interfaces:**
- Consumes: `NodeConfig.token` (String)
- Produces: `AuthInterceptor { new(token: &str) }` que implementa `tonic::service::Interceptor` — retorna `Unauthenticated` se header `authorization` != `"Bearer <token>"`

- [ ] **Step 1: Escrever testes do interceptor**

Substituir o conteúdo de `crates/node/src/interceptor.rs`:

```rust
use tonic::{Request, Status};

#[derive(Clone)]
pub struct AuthInterceptor {
    expected: String,
}

impl AuthInterceptor {
    pub fn new(token: &str) -> Self {
        Self { expected: format!("Bearer {}", token) }
    }
}

impl tonic::service::Interceptor for AuthInterceptor {
    fn call(&mut self, req: Request<()>) -> Result<Request<()>, Status> {
        let provided = req
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if provided == self.expected {
            Ok(req)
        } else {
            Err(Status::unauthenticated("invalid or missing token"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::metadata::MetadataValue;

    fn req_with_token(token: &str) -> Request<()> {
        let mut req = Request::new(());
        req.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from(token).unwrap(),
        );
        req
    }

    #[test]
    fn valid_token_passes() {
        let mut interceptor = AuthInterceptor::new("secret-token");
        let req = req_with_token("Bearer secret-token");
        assert!(interceptor.call(req).is_ok());
    }

    #[test]
    fn invalid_token_rejected() {
        let mut interceptor = AuthInterceptor::new("secret-token");
        let req = req_with_token("Bearer wrong-token");
        let err = interceptor.call(req).unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn missing_token_rejected() {
        let mut interceptor = AuthInterceptor::new("secret-token");
        let req = Request::new(());
        let err = interceptor.call(req).unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn token_without_bearer_prefix_rejected() {
        let mut interceptor = AuthInterceptor::new("secret-token");
        let req = req_with_token("secret-token");
        let err = interceptor.call(req).unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }
}
```

- [ ] **Step 2: Rodar testes para confirmar que passam**

Run: `cargo test -p oxy-node interceptor::tests 2>&1 | tail -10`

Expected:
```
test interceptor::tests::valid_token_passes ... ok
test interceptor::tests::invalid_token_rejected ... ok
test interceptor::tests::missing_token_rejected ... ok
test interceptor::tests::token_without_bearer_prefix_rejected ... ok
test result: ok. 4 passed; 0 failed
```

- [ ] **Step 3: Commit**

```bash
git add crates/node/src/interceptor.rs
git commit -m "feat(oxy-node): auth interceptor validates Bearer token"
```

---

### Task 7: gRPC Server — NodeServiceImpl

**Files:**
- Modify: `crates/node/src/server.rs`

**Interfaces:**
- Consumes: `DockerBackend`, `MockDockerBackend` (cfg test), `forward_logs` de `stream.rs`, todos os tipos proto de `oxy_core::proto::node`
- Produces: `NodeServiceImpl<B: DockerBackend>` que implementa `node_service_server::NodeService` com todos os 6 RPCs

- [ ] **Step 1: Escrever testes primeiro**

Substituir o conteúdo de `crates/node/src/server.rs`:

```rust
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use oxy_core::proto::node::{
    node_service_server::NodeService,
    LogLine, ServerCommandRequest, ServerDeleteRequest, ServerLogsRequest,
    ServerReply, ServerStartRequest, ServerStats, ServerStatsRequest,
    ServerStopRequest,
};
use crate::docker::DockerBackend;
use crate::stream::forward_logs;

pub struct NodeServiceImpl<B: DockerBackend> {
    docker: Arc<B>,
}

impl<B: DockerBackend> NodeServiceImpl<B> {
    pub fn new(docker: Arc<B>) -> Self {
        Self { docker }
    }

    fn ok(message: impl Into<String>) -> Response<ServerReply> {
        Response::new(ServerReply { success: true, message: message.into() })
    }
}

#[async_trait]
impl<B: DockerBackend> NodeService for NodeServiceImpl<B> {
    type StreamLogsStream = ReceiverStream<Result<LogLine, Status>>;

    async fn start_server(
        &self,
        req: Request<ServerStartRequest>,
    ) -> Result<Response<ServerReply>, Status> {
        let id = req.into_inner().server_id;
        self.docker
            .start_container(id.clone())
            .await
            .map_err(Status::from)?;
        Ok(Self::ok(format!("started {}", id)))
    }

    async fn stop_server(
        &self,
        req: Request<ServerStopRequest>,
    ) -> Result<Response<ServerReply>, Status> {
        let r = req.into_inner();
        self.docker
            .stop_container(r.server_id.clone(), r.timeout)
            .await
            .map_err(Status::from)?;
        Ok(Self::ok(format!("stopped {}", r.server_id)))
    }

    async fn delete_server(
        &self,
        req: Request<ServerDeleteRequest>,
    ) -> Result<Response<ServerReply>, Status> {
        let id = req.into_inner().server_id;
        self.docker
            .delete_container(id.clone())
            .await
            .map_err(Status::from)?;
        Ok(Self::ok(format!("deleted {}", id)))
    }

    async fn get_stats(
        &self,
        req: Request<ServerStatsRequest>,
    ) -> Result<Response<ServerStats>, Status> {
        let id = req.into_inner().server_id;
        let s = self.docker.get_stats(id.clone()).await.map_err(Status::from)?;
        Ok(Response::new(ServerStats {
            server_id:    id,
            memory_bytes: s.memory_bytes,
            cpu_percent:  s.cpu_percent,
            rx_bytes:     s.rx_bytes,
            tx_bytes:     s.tx_bytes,
        }))
    }

    async fn stream_logs(
        &self,
        req: Request<ServerLogsRequest>,
    ) -> Result<Response<Self::StreamLogsStream>, Status> {
        let r = req.into_inner();
        let (tx, rx) = mpsc::channel(32);
        let docker = Arc::clone(&self.docker);

        tokio::spawn(async move {
            match docker.log_stream(r.server_id, r.follow).await {
                Ok(stream) => forward_logs(stream, tx).await,
                Err(e) => {
                    let _ = tx.send(Err(Status::from(e))).await;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn send_command(
        &self,
        req: Request<ServerCommandRequest>,
    ) -> Result<Response<ServerReply>, Status> {
        let r = req.into_inner();
        self.docker
            .send_command(r.server_id.clone(), r.content)
            .await
            .map_err(Status::from)?;
        Ok(Self::ok(format!("command sent to {}", r.server_id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::{ContainerStats, LogChunk, MockDockerBackend};
    use crate::error::NodeError;
    use futures_util::stream;

    fn svc(mock: MockDockerBackend) -> NodeServiceImpl<MockDockerBackend> {
        NodeServiceImpl::new(Arc::new(mock))
    }

    #[tokio::test]
    async fn start_server_delegates_to_docker() {
        let mut mock = MockDockerBackend::new();
        mock.expect_start_container()
            .withf(|id| id == "srv-1")
            .once()
            .returning(|_| Ok(()));

        let reply = svc(mock)
            .start_server(Request::new(ServerStartRequest { server_id: "srv-1".into() }))
            .await
            .unwrap()
            .into_inner();

        assert!(reply.success);
    }

    #[tokio::test]
    async fn stop_server_passes_timeout() {
        let mut mock = MockDockerBackend::new();
        mock.expect_stop_container()
            .withf(|id, t| id == "srv-1" && *t == 30)
            .once()
            .returning(|_, _| Ok(()));

        svc(mock)
            .stop_server(Request::new(ServerStopRequest {
                server_id: "srv-1".into(),
                timeout:   30,
            }))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn delete_server_delegates_to_docker() {
        let mut mock = MockDockerBackend::new();
        mock.expect_delete_container()
            .withf(|id| id == "srv-1")
            .once()
            .returning(|_| Ok(()));

        svc(mock)
            .delete_server(Request::new(ServerDeleteRequest { server_id: "srv-1".into() }))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn get_stats_maps_to_proto() {
        let mut mock = MockDockerBackend::new();
        mock.expect_get_stats()
            .once()
            .returning(|_| Ok(ContainerStats {
                memory_bytes: 1024,
                cpu_percent:  5.0,
                rx_bytes:     100,
                tx_bytes:     200,
            }));

        let stats = svc(mock)
            .get_stats(Request::new(ServerStatsRequest { server_id: "srv-1".into() }))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(stats.memory_bytes, 1024);
        assert!((stats.cpu_percent - 5.0).abs() < 0.001);
        assert_eq!(stats.rx_bytes,  100);
        assert_eq!(stats.tx_bytes,  200);
    }

    #[tokio::test]
    async fn send_command_delegates_to_docker() {
        let mut mock = MockDockerBackend::new();
        mock.expect_send_command()
            .withf(|id, cmd| id == "srv-1" && cmd == "say hello")
            .once()
            .returning(|_, _| Ok(()));

        svc(mock)
            .send_command(Request::new(ServerCommandRequest {
                server_id: "srv-1".into(),
                content:   "say hello".into(),
            }))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn stream_logs_forwards_chunks_to_client() {
        use tokio_stream::StreamExt;

        let mut mock = MockDockerBackend::new();
        mock.expect_log_stream()
            .once()
            .returning(|_, _| {
                let chunks = vec![
                    Ok(LogChunk { content: "hello\n".into(), stream: "stdout".into() }),
                    Ok(LogChunk { content: "world\n".into(), stream: "stdout".into() }),
                ];
                Ok(Box::pin(stream::iter(chunks)))
            });

        let mut response = svc(mock)
            .stream_logs(Request::new(ServerLogsRequest {
                server_id: "srv-1".into(),
                follow:    false,
            }))
            .await
            .unwrap()
            .into_inner();

        let line1 = response.next().await.unwrap().unwrap();
        assert_eq!(line1.content, "hello\n");

        let line2 = response.next().await.unwrap().unwrap();
        assert_eq!(line2.content, "world\n");
    }

    #[tokio::test]
    async fn start_server_returns_grpc_error_on_not_found() {
        let mut mock = MockDockerBackend::new();
        mock.expect_start_container()
            .once()
            .returning(|_| Err(NodeError::NotFound("srv-x".into())));

        let err = svc(mock)
            .start_server(Request::new(ServerStartRequest { server_id: "srv-x".into() }))
            .await
            .unwrap_err();

        assert_eq!(err.code(), tonic::Code::NotFound);
    }
}
```

- [ ] **Step 2: Rodar testes**

Run: `cargo test -p oxy-node server::tests 2>&1 | tail -15`

Expected:
```
test server::tests::start_server_delegates_to_docker ... ok
test server::tests::stop_server_passes_timeout ... ok
test server::tests::delete_server_delegates_to_docker ... ok
test server::tests::get_stats_maps_to_proto ... ok
test server::tests::send_command_delegates_to_docker ... ok
test server::tests::stream_logs_forwards_chunks_to_client ... ok
test server::tests::start_server_returns_grpc_error_on_not_found ... ok
test result: ok. 7 passed; 0 failed
```

- [ ] **Step 3: Commit**

```bash
git add crates/node/src/server.rs
git commit -m "feat(oxy-node): gRPC NodeServiceImpl with all 6 RPCs and tests"
```

---

### Task 8: Wire lib.rs — servidor tonic real

**Files:**
- Modify: `crates/node/src/lib.rs` — substituir stub por servidor tonic real com interceptor

**Interfaces:**
- Consumes: `BollardDocker::connect()`, `AuthInterceptor::new()`, `NodeServiceImpl::new()`, `NodeConfig` de `oxy_core`
- Produces: `run(config: NodeConfig) -> oxy_core::Result<()>` que inicia o servidor gRPC na porta configurada

- [ ] **Step 1: Escrever teste de integração**

Adicionar ao final de `crates/node/src/lib.rs` (antes do `run`):

```rust
#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use oxy_core::proto::node::{
        node_service_client::NodeServiceClient,
        node_service_server::NodeServiceServer,
        ServerStartRequest,
    };
    use tonic::transport::Server;
    use tokio_stream::wrappers::TcpListenerStream;
    use crate::docker::MockDockerBackend;
    use crate::server::NodeServiceImpl;

    #[tokio::test]
    async fn integration_grpc_start_server_round_trip() {
        let mut mock = MockDockerBackend::new();
        mock.expect_start_container()
            .once()
            .returning(|_| Ok(()));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let svc = NodeServiceImpl::new(Arc::new(mock));
        tokio::spawn(async move {
            Server::builder()
                .add_service(NodeServiceServer::new(svc))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        });

        // Pequena pausa para o servidor subir
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = NodeServiceClient::connect(format!("http://{}", addr))
            .await
            .unwrap();

        let reply = client
            .start_server(ServerStartRequest { server_id: "test-srv".into() })
            .await
            .unwrap()
            .into_inner();

        assert!(reply.success);
        assert!(reply.message.contains("test-srv"));
    }
}
```

- [ ] **Step 2: Rodar teste de integração para confirmar que passa**

Run: `cargo test -p oxy-node tests::integration_grpc_start_server_round_trip 2>&1 | tail -5`

Expected: `test tests::integration_grpc_start_server_round_trip ... ok`

- [ ] **Step 3: Implementar run() real**

Substituir `crates/node/src/lib.rs` completo:

```rust
pub mod docker;
pub mod error;
pub mod interceptor;
pub mod server;
pub mod stream;

use std::sync::Arc;
use oxy_core::{NodeConfig, OxyError};
use oxy_core::proto::node::node_service_server::NodeServiceServer;
use crate::docker::BollardDocker;
use crate::interceptor::AuthInterceptor;
use crate::server::NodeServiceImpl;

pub async fn run(config: NodeConfig) -> oxy_core::Result<()> {
    let addr = config
        .grpc_listen
        .parse()
        .map_err(|e: std::net::AddrParseError| OxyError::Config(e.to_string()))?;

    let docker = BollardDocker::connect()
        .map_err(|e| OxyError::Config(e.to_string()))?;

    let interceptor = AuthInterceptor::new(&config.token);
    let service = NodeServiceImpl::new(Arc::new(docker));

    tracing::info!(listen = %config.grpc_listen, "node starting");

    tonic::transport::Server::builder()
        .add_service(NodeServiceServer::with_interceptor(service, interceptor))
        .serve(addr)
        .await
        .map_err(|e| OxyError::Config(e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use oxy_core::proto::node::{
        node_service_client::NodeServiceClient,
        node_service_server::NodeServiceServer,
        ServerStartRequest,
    };
    use tonic::transport::Server;
    use tokio_stream::wrappers::TcpListenerStream;
    use crate::docker::MockDockerBackend;
    use crate::server::NodeServiceImpl;

    #[tokio::test]
    async fn integration_grpc_start_server_round_trip() {
        let mut mock = MockDockerBackend::new();
        mock.expect_start_container()
            .once()
            .returning(|_| Ok(()));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let svc = NodeServiceImpl::new(Arc::new(mock));
        tokio::spawn(async move {
            Server::builder()
                .add_service(NodeServiceServer::new(svc))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = NodeServiceClient::connect(format!("http://{}", addr))
            .await
            .unwrap();

        let reply = client
            .start_server(ServerStartRequest { server_id: "test-srv".into() })
            .await
            .unwrap()
            .into_inner();

        assert!(reply.success);
        assert!(reply.message.contains("test-srv"));
    }
}
```

- [ ] **Step 4: Rodar todos os testes do workspace**

Run: `cargo test 2>&1 | tail -15`

Expected: todos os testes passando (4 config core + 8 docker + 3 stream + 4 interceptor + 7 server + 1 integração = 27 testes).

- [ ] **Step 5: Build release para confirmar que binário ainda compila**

Run: `cargo build --release 2>&1 | tail -5`

Expected: `Finished release` — sem warnings fatais.

- [ ] **Step 6: Commit e push**

```bash
git add crates/node/src/lib.rs
git commit -m "feat(oxy-node): wire real tonic gRPC server with auth interceptor"
git push origin main
```

---

## Self-Review

**Spec coverage (Seção 3 do design):**

| Requisito da Spec | Task |
|---|---|
| `DockerBackend` trait mockável via mockall | Task 1 |
| Código de negócio nunca chama bollard diretamente | Tasks 2–5 (via trait) |
| Container com `open_stdin: true`, `stdin_once: false` | Task 2 (`create_container`) |
| Limites `memory_mb` e `cpu_percent` obrigatórios | Task 2 (validação em `create_container`) |
| `SendCommand` via stdin attach (nunca docker exec) | Task 3 |
| `StreamLogs` com break em `tx.send().is_err()` | Task 4 (`forward_logs`) |
| Streaming `bollard::logs()` → tokio channel → gRPC | Tasks 4 + 7 |
| `GetStats` com CPU%, memória, rx/tx bytes | Task 5 |
| `AuthInterceptor` valida `Bearer <token>` no metadata | Task 6 |
| `NodeServiceImpl` com os 6 RPCs do proto | Task 7 |
| `run()` real com tonic server + interceptor | Task 8 |

**Não coberto neste plano (intencionalmente):**
- `create_container` chamado pelo panel na instalação (Plans 3/4)
- Reconexão automática ao Docker daemon após restart
- Pull de imagem antes de criar container (Plan 4 — Eggs)

**Placeholder scan:** Nenhum TBD ou step sem código. Os `unimplemented!()` em Task 2 são substituídos em Tasks 3–5.

**Type consistency:**
- `LogChunk { content: String, stream: String }` definido em Task 1, usado em Tasks 4 e 7
- `ContainerStats { memory_bytes, cpu_percent, rx_bytes, tx_bytes }` definido em Task 1, mapeado para proto em Task 7
- `NodeError` → `tonic::Status` via `From` definido em Task 1, usado em Task 7
- `BoxStream<'static, Result<LogChunk>>` retornado por `log_stream` em Task 4, consumido em Task 7
- `ReceiverStream<Result<LogLine, Status>>` como `StreamLogsStream` associado em Task 7
