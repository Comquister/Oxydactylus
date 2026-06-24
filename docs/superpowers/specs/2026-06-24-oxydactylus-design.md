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
    egg_id       UUID REFERENCES eggs(id),
    name         TEXT NOT NULL,
    docker_image TEXT NOT NULL,   -- imagem escolhida dentre docker_images do egg
    memory_mb    INT NOT NULL,
    cpu_percent  INT NOT NULL,
    env_vars     JSONB NOT NULL DEFAULT '{}',   -- valores das variáveis do egg (resolvidos)
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

## Seção 6 — Eggs: Templates de Servidor de Jogo

### Conceito

Eggs são templates reutilizáveis que definem tudo o que o panel e o node precisam para instalar e rodar um tipo de servidor de jogo: imagem Docker, comando de inicialização, variáveis de ambiente, script de instalação e patches de arquivos de configuração.

### Formato nativo `.toml`

```toml
[egg]
name          = "Purpur"
author        = "purpur@birdflop.com"
description   = "Drop-in replacement for Paper with extra configurability."
features      = ["eula", "java_version", "pid_limit"]
file_denylist = []

[startup]
command   = "java {{JVM_EXTRA}} -jar {{SERVER_JARFILE}}"
stop      = "stop"
detection = ")! For help, type "   # string nos logs que indica servidor pronto

[docker_images]
"Java 21" = "ghcr.io/ptero-eggs/yolks:java_21"
"Java 17" = "ghcr.io/ptero-eggs/yolks:java_17"
"Java 11" = "ghcr.io/ptero-eggs/yolks:java_11"
"Java 8"  = "ghcr.io/ptero-eggs/yolks:java_8"

[[variables]]
name          = "Minecraft Version"
env_variable  = "MINECRAFT_VERSION"
description   = "The version of Minecraft to download."
default       = "latest"
user_viewable = true
user_editable = true
rules         = "required|string|max:20"
field_type    = "text"

[[variables]]
name          = "Server Jar File"
env_variable  = "SERVER_JARFILE"
description   = "The name of the .jar file to run."
default       = "server.jar"
user_viewable = true
user_editable = true
rules         = "required|regex:/^([\\w\\d._-]+)(\\.jar)$/|max:80"
field_type    = "text"

[[variables]]
name          = "JVM Arguments"
env_variable  = "JVM_EXTRA"
description   = "Argumentos adicionais para a JVM."
default       = ""
user_viewable = true
user_editable = true
rules         = "nullable|string"
field_type    = "text"

[install]
container  = "ghcr.io/ptero-eggs/installers:alpine"
entrypoint = "ash"
script     = """
#!/bin/ash
# script de instalação aqui
"""

[[config_files]]
path   = "server.properties"
parser = "properties"
[config_files.patches]
"server-ip"   = "0.0.0.0"
"server-port" = "{{server.build.default.port}}"
```

`{{server.build.default.port}}` é uma **variável de contexto do servidor** (porta alocada pelo panel), distinta das `{{VAR}}` definidas pelo usuário.

### Schema PostgreSQL

```sql
CREATE TABLE eggs (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name          TEXT NOT NULL,
    description   TEXT,
    author        TEXT,
    version       TEXT NOT NULL DEFAULT '1.0.0',
    features      TEXT[]  NOT NULL DEFAULT '{}',      -- ["eula", "java_version"]
    file_denylist TEXT[]  NOT NULL DEFAULT '{}',
    docker_images JSONB   NOT NULL DEFAULT '{}',      -- {"Java 21": "ghcr.io/..."}
    start_cmd     TEXT NOT NULL,
    stop_cmd      TEXT NOT NULL DEFAULT 'stop',
    startup_done  TEXT,                               -- string nos logs = servidor pronto
    created_at    TIMESTAMPTZ DEFAULT NOW(),
    updated_at    TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE egg_variables (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    egg_id        UUID NOT NULL REFERENCES eggs(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,            -- label: "Minecraft Version"
    description   TEXT,
    env_variable  TEXT NOT NULL,            -- nome real: "MINECRAFT_VERSION"
    default_val   TEXT,
    user_viewable BOOLEAN NOT NULL DEFAULT TRUE,
    user_editable BOOLEAN NOT NULL DEFAULT TRUE,
    rules         TEXT,                     -- "required|string|max:20"
    field_type    TEXT NOT NULL DEFAULT 'text'
);

CREATE TABLE egg_install_scripts (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    egg_id     UUID NOT NULL REFERENCES eggs(id) ON DELETE CASCADE,
    container  TEXT NOT NULL,              -- imagem do instalador
    entrypoint TEXT NOT NULL DEFAULT 'bash',
    script     TEXT NOT NULL
);

CREATE TABLE egg_config_files (
    id      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    egg_id  UUID NOT NULL REFERENCES eggs(id) ON DELETE CASCADE,
    path    TEXT NOT NULL,                -- caminho relativo dentro do container
    parser  TEXT NOT NULL CHECK (parser IN ('properties','json','yaml','ini','xml')),
    patches JSONB NOT NULL                -- {"chave": "valor_ou_{{VAR}}"}
);
```

### Validação de `rules`

Subconjunto implementado em Rust (suficiente para cobrir todos os eggs oficiais):

| Rule | Comportamento |
|---|---|
| `required` | campo não pode ser vazio |
| `nullable` | campo pode ser null/vazio |
| `string` | valor é string |
| `integer` | valor é inteiro |
| `boolean` | valor é true/false |
| `max:N` | comprimento máximo N |
| `min:N` | comprimento mínimo N |
| `regex:/pattern/` | valor deve casar com regex |

### Importador de eggs Pterodactyl

`POST /api/eggs/import` aceita o JSON do formato `PTDL_v2` e converte automaticamente:

| Campo Pterodactyl | Campo Oxydactylus |
|---|---|
| `name` | `eggs.name` |
| `author` | `eggs.author` |
| `description` | `eggs.description` |
| `features` | `eggs.features` |
| `file_denylist` | `eggs.file_denylist` |
| `docker_images` | `eggs.docker_images` (JSONB) |
| `startup` | `eggs.start_cmd` |
| `config.stop` | `eggs.stop_cmd` |
| `config.startup.done` | `eggs.startup_done` |
| `config.files` (JSON string) | `egg_config_files` (parser + patches) |
| `variables[].name` | `egg_variables.name` |
| `variables[].env_variable` | `egg_variables.env_variable` |
| `variables[].default_value` | `egg_variables.default_val` |
| `variables[].rules` | `egg_variables.rules` |
| `variables[].user_viewable` | `egg_variables.user_viewable` |
| `variables[].user_editable` | `egg_variables.user_editable` |
| `scripts.installation.script` | `egg_install_scripts.script` |
| `scripts.installation.container` | `egg_install_scripts.container` |
| `scripts.installation.entrypoint` | `egg_install_scripts.entrypoint` |

### Export como `.toml`

`GET /api/eggs/:id/export` serializa o egg do banco para o formato `.toml` nativo — pronto para versionar em git ou compartilhar entre instâncias.

### Substituição de variáveis em runtime

Antes de criar o container, o panel resolve todas as `{{VAR}}` combinando:

1. Valores definidos pelo usuário para o servidor
2. Defaults do egg para variáveis não preenchidas
3. Variáveis de contexto do servidor (`{{server.build.default.port}}` etc.)

O container recebe apenas variáveis resolvidas como `env` — nunca strings com `{{}}` literais.

O node aplica os `patches` de `egg_config_files` após a instalação, usando o `parser` correto para cada arquivo (properties, json, yaml, etc.) antes de iniciar o servidor.

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
