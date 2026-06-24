# Oxydactylus — Design Spec

**Data:** 2026-06-24
**Status:** Aprovado

---

## Visão Geral

Oxydactylus é um substituto do Pterodactyl escrito inteiramente em Rust. Distribui-se como um único binário estático que opera como **panel** (web UI + API), **node** (daemon de containers) ou **both** (ambos simultaneamente), conforme o campo `[role]` do `config.toml`.

---

## Seção 1 — Arquitetura Geral

### Binário único, três roles

```toml
# Panel
[role]
type         = "panel"
http_listen  = "0.0.0.0:3000"
database_url = "postgres://..."

# Node
[role]
type        = "node"
grpc_listen = "0.0.0.0:8080"

# Ambos (deploy homogêneo / desenvolvimento local)
[role]
type         = "both"
http_listen  = "0.0.0.0:3000"
grpc_listen  = "0.0.0.0:8080"
database_url = "postgres://..."
```

O entrypoint (`cli`) lê a config e despacha:

```rust
match config.role.r#type {
    Role::Panel => panel::run(config).await?,
    Role::Node  => node::run(config).await?,
    Role::Both  => {
        tokio::join!(panel::run(config.clone()), node::run(config.clone()));
    }
}
```

`tokio::join!` mantém os dois runtimes vivos concorrentemente. `tokio::select!` não é usado aqui pois cancelaria o primeiro que terminasse.

### Fluxo de dados

```
Browser (WASM/Leptos)
    │  WebSocket + HTTP
    ▼
Panel (Leptos SSR + Axum)
    │  gRPC multiplexado (tonic + tower pool)
    ▼
Node (daemon Rust)
    │  Unix socket (bollard → Docker daemon)
    ▼
PID 1 stdin do container de jogo
```

### Workspace Cargo

```
oxydactylus/
├── Cargo.toml          # [workspace]
├── crates/
│   ├── core/           # tipos compartilhados, config, schemas proto
│   │   ├── build.rs    # tonic-build compila os .proto
│   │   ├── proto/
│   │   │   └── oxydactylus.proto
│   │   └── src/
│   │       ├── lib.rs
│   │       └── proto.rs  # pub mod node { include!(concat!(env!("OUT_DIR"), "...")) }
│   ├── panel/
│   │   ├── migrations/
│   │   └── src/
│   ├── node/
│   │   └── src/
│   └── cli/
│       └── src/
│           └── main.rs
└── docs/
```

### Geração de código proto

`core/build.rs` usa `tonic-build` para compilar os `.proto` em `OUT_DIR`. O crate `core` re-exporta todos os tipos gerados via `core::proto::*`. Nenhum outro crate acessa `OUT_DIR` diretamente.

```rust
// core/src/proto.rs
pub mod node {
    tonic::include_proto!("oxydactylus.node");
}
```

---

## Seção 2 — Dados e Autenticação

### Schema PostgreSQL

```sql
-- Usuários do panel
CREATE TABLE users (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email        TEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,          -- argon2id
    role         TEXT NOT NULL,           -- 'admin' | 'subuser'
    created_at   TIMESTAMPTZ DEFAULT NOW()
);

-- Refresh tokens (JWT curto + refresh persistido)
CREATE TABLE refresh_tokens (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash  TEXT NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL,
    revoked_at  TIMESTAMPTZ             -- NULL = válido
);

CREATE INDEX idx_refresh_tokens_lookup
    ON refresh_tokens (token_hash)
    WHERE revoked_at IS NULL AND expires_at > NOW();

-- Nós registrados
CREATE TABLE nodes (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT NOT NULL,
    fqdn       TEXT NOT NULL,
    grpc_port  INT NOT NULL DEFAULT 8080,
    token_hash TEXT NOT NULL,           -- token que o panel envia ao node via gRPC
    is_online  BOOLEAN NOT NULL DEFAULT FALSE
);

-- Instâncias de servidores de jogos
CREATE TABLE servers (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    node_id      UUID NOT NULL REFERENCES nodes(id),
    owner_id     UUID NOT NULL REFERENCES users(id),
    name         TEXT NOT NULL,
    docker_image TEXT NOT NULL,
    memory_mb    INT NOT NULL,
    cpu_percent  INT NOT NULL,
    env_vars     JSONB NOT NULL DEFAULT '{}',   -- flexível por jogo, sem schema fixo
    status       TEXT NOT NULL DEFAULT 'stopped'
        CHECK (status IN ('installing','running','stopped','error'))
);
```

### Fluxo de autenticação — usuários

| Etapa | Endpoint | Detalhe |
|---|---|---|
| Login | `POST /auth/login` | valida senha (argon2id), emite access JWT (15min) + refresh token (30d, hash no banco) |
| Request autenticado | header `Authorization: Bearer <access>` | middleware valida JWT sem banco |
| Renovação | `POST /auth/refresh` | valida refresh token contra hash + `revoked_at IS NULL` + `expires_at > NOW()` |
| Logout | `POST /auth/logout` | seta `revoked_at = NOW()` no token atual |

### Autenticação Panel → Node (gRPC)

O Panel é o **cliente gRPC**; o Node é o **servidor gRPC**. O Panel envia o token do nó no metadata gRPC:

```rust
// panel: injeta token no metadata de cada chamada
let mut req = Request::new(payload);
req.metadata_mut().insert("authorization", token.parse()?);
```

O Node valida via **tonic Interceptor**, comparando contra o hash em `config.toml` local — sem roundtrip ao banco do panel.

### Autenticação Node → Panel (eventos HTTP)

Se o node precisar notificar o panel de forma assíncrona (ex: servidor caiu), chama `POST /internal/events` com o token no header. O panel valida o hash no banco.

---

## Seção 3 — Node: Gerenciamento Docker e Streaming

### Estrutura do crate `node`

```
node/src/
├── lib.rs
├── server.rs        ← implementação gRPC (tonic)
├── docker.rs        ← trait DockerBackend + impl bollard
├── stream.rs        ← streaming logs com break em pipe quebrado
└── interceptor.rs   ← tonic Interceptor: valida token da config
```

### Contrato gRPC

```protobuf
service NodeService {
    rpc StartServer   (ServerStartRequest)   returns (ServerReply);
    rpc StopServer    (ServerStopRequest)    returns (ServerReply);
    rpc DeleteServer  (ServerDeleteRequest)  returns (ServerReply);
    rpc GetStats      (ServerStatsRequest)   returns (ServerStats);
    rpc StreamLogs    (ServerLogsRequest)    returns (stream LogLine);
    rpc SendCommand   (ServerCommandRequest) returns (ServerReply);
}
```

### Ciclo de vida de um servidor de jogo

| Operação | Implementação |
|---|---|
| Instalar | pull imagem Docker + criar container via bollard |
| Iniciar | `docker start` + attach stdout/stderr |
| Streaming de logs | `bollard::logs()` → tokio channel → gRPC server-stream |
| Enviar comando | **attach stdin PID 1** + write `"cmd\n"` (nunca `docker exec`) |
| Parar | SIGTERM → timeout configurável → SIGKILL |
| Deletar | remove container + volumes |

### Container: criação com stdin aberto

```rust
Config {
    open_stdin: Some(true),
    stdin_once: Some(false),   // stdin permanece aberto após primeiro attach
    host_config: Some(HostConfig {
        memory: Some(server.memory_mb * 1024 * 1024),
        nano_cpus: Some(server.cpu_percent as i64 * 10_000_000),
        ..Default::default()
    }),
    ..Default::default()
}
```

Servidores sem `memory_mb` e `cpu_percent` definidos são **rejeitados** antes da criação.

### SendCommand — stdin attach (não docker exec)

```rust
let mut attach = docker.attach_container(&id, Some(AttachContainerOptions::<String> {
    stdin: Some(true),
    stream: Some(true),
    ..Default::default()
})).await?;

attach.input.send(format!("{}\n", command).into()).await?;
```

### StreamLogs — resiliência a desconexões

```rust
while let Some(chunk) = log_stream.next().await {
    let line = LogLine { content: chunk?.to_string() };
    if tx.send(Ok(line)).await.is_err() {
        break;  // panel fechou o stream: encerra task imediatamente, sem vazar memória
    }
}
// bollard stream dropado aqui automaticamente
```

### Abstração testável

```rust
#[cfg_attr(test, mockall::automock)]
trait DockerBackend {
    async fn start_container(&self, id: &str) -> Result<()>;
    async fn stop_container(&self, id: &str, timeout: u32) -> Result<()>;
}
```

Código de negócio nunca chama `bollard` diretamente — apenas via trait.

---

## Seção 4 — Panel: Leptos SSR, API REST e Proxy WebSocket

### Estrutura do crate `panel`

```
panel/src/
├── lib.rs
├── app.rs              ← Leptos root component + router
├── api/
│   ├── auth.rs         ← /auth/login, /refresh, /logout
│   ├── servers.rs      ← CRUD servidores
│   ├── nodes.rs        ← CRUD nós
│   └── internal.rs     ← /internal/events (webhooks dos nodes)
├── ws/
│   └── proxy.rs        ← WebSocket ↔ gRPC proxy
├── grpc/
│   └── client.rs       ← DashMap<NodeId, NodeClient> com reconexão
└── components/
    ├── console.rs       ← island Leptos (hidratação sob demanda)
    ├── dashboard.rs
    └── servers.rs
```

### Stack

| Camada | Crate |
|---|---|
| HTTP | `axum` |
| SSR + WASM | `leptos` + `leptos_axum` |
| gRPC client | `tonic` + tower connection pool |
| WebSocket | `axum::extract::WebSocketUpgrade` |

### Proxy WebSocket → gRPC

```
Browser abre console:
  Browser ──ws connect──▶ Panel ──gRPC StreamLogs──▶ Node ──bollard──▶ container stdout
         ◀──ws text──────        ◀──LogLine proto──

Browser envia comando:
  Browser ──ws text──▶ Panel ──tokio::spawn(gRPC SendCommand)──▶ Node ──stdin attach──▶ PID 1
```

```rust
async fn ws_handler(ws: WebSocketUpgrade, node_client: NodeClient) -> impl IntoResponse {
    ws.on_upgrade(|mut socket| async move {
        let mut log_stream = node_client.clone().stream_logs(request).await?.into_inner();
        loop {
            tokio::select! {
                Some(line) = log_stream.next() => {
                    if socket.send(Message::Text(line?.content)).await.is_err() { break; }
                }
                Some(Ok(msg)) = socket.next() => {
                    if let Message::Text(cmd) = msg {
                        let mut client = node_client.clone(); // clone barato (Arc interno)
                        tokio::spawn(async move {
                            client.send_command(ServerCommandRequest { content: cmd }).await.ok();
                        });
                    }
                }
                else => break,
            }
        }
    })
}
```

`send_command` em `tokio::spawn` separado — um gRPC lento não bloqueia o recebimento de logs.

### Pool gRPC por node

`DashMap<NodeId, NodeClient>` — uma conexão persistente por node. Reconexão automática com backoff exponencial. `NodeClient::clone()` é O(1) (clona o `Arc` do tower).

### Leptos SSR vs Islands

- Rotas estáticas (dashboard, listagem): renderizadas no servidor, HTML puro, carregamento mínimo
- Componentes interativos (console, gráficos): marcados com `#[component(island)]` — servidor envia shell HTML, WASM hidrata apenas o island no browser. O servidor nunca tenta renderizar terminal ou xterm antes do WASM assumir.

---

## Seção 5 — Testes e Estratégia de Build

### Pirâmide de testes

```
        [E2E]          ← Playwright: fluxos críticos browser WASM
       [integração]    ← testcontainers-rs: postgres real
      [unitários]      ← Rust nativo, mocks nas bordas de I/O
```

### Testes unitários

Mocks via `mockall` apenas nas traits de borda (`DockerBackend`, `NodeGrpcClient`). Zero mocks de banco — banco real em integração.

### Testes de integração

```rust
#[tokio::test]
async fn test_auth_refresh_token() {
    let pg = Postgres::default().start().await;
    let pool = connect(&pg.connection_string()).await;
    sqlx::migrate!().run(&pool).await.unwrap();
    // testa fluxo real contra banco real
}
```

Se uma query quebrar com schema errado, a build falha antes do deploy.

### Testes E2E (Playwright)

Fluxos obrigatórios cobertos:

1. Login → dashboard → criar servidor → iniciar → enviar comando → ver log no console
2. Refresh token expirado → redirect para login
3. Node offline → panel exibe status correto

### Perfil de release

```toml
[profile.release]
opt-level     = 3
lto           = "thin"
codegen-units = 1
strip         = true
```

### Build WASM + assets embutidos

```bash
cargo leptos build --release
```

Assets WASM embutidos no binário do panel via `include_dir!()` em compile-time. **Deploy = copiar um único arquivo.**

### Target de distribuição

```
oxydactylus-linux-x86_64   # musl, binário estático, ~20-40MB com WASM embutido
```

Roda em qualquer Linux sem glibc, sem runtime, sem dependências além do Docker daemon.

### CI (GitHub Actions)

```
push → cargo test (todos os crates) → cargo leptos build → docker build → release tag
```

---

## Topologia Final

```
[Browser WASM]
    ↕ WebSocket / JSON
[Panel — Axum + Leptos SSR]
    ↕ gRPC multiplexado (tonic + tower pool)
[Node Daemon]
    ↕ Unix socket (bollard → Docker daemon)
[PID 1 stdin do container]
```

| Fronteira | Protocolo | Autenticação |
|---|---|---|
| Browser → Panel | WebSocket + HTTP | JWT access token |
| Panel → Node | gRPC (tonic) | token estático no metadata, validado por Interceptor no node |
| Node → Panel | HTTP REST | token estático no header, hash validado no banco |
| Node → Docker | Unix socket (bollard) | sem auth (socket local) |
