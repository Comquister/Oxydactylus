# Ownership + Subusers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Consolidar as 4 migrações em um schema limpo, adicionar propriedade de servidores por usuário e um sistema completo de permissões de subusers compatível com Pterodactyl.

**Architecture:** Schema compactado em `001_initial.sql`; `servers.user_id NOT NULL` referencia o dono; helper `check_server_access` verifica admin > dono > subuser com permissão; `subusers.rs` gerencia o CRUD de subusers com `permissions TEXT[]`; permissões definidas como constantes em `permissions.rs`.

**Tech Stack:** Rust 2021, axum 0.7, sqlx 0.8 (PostgreSQL), tonic 0.12, tokio, thiserror 2

## Global Constraints

- Rust edition 2021, workspace resolver = "2"
- Substituir 001–004 por `001_initial.sql` — nenhum dado de produção existe
- `PanelError::Forbidden` → HTTP 403 (já existe em `error.rs`)
- Permissões: strings `"grupo.acao"` em lowercase, validadas contra `crate::permissions::ALL_PERMISSIONS`
- Testes: `#[sqlx::test(migrations = "./migrations")]` — requer `DATABASE_URL`
- `AdminUser(pub AuthUser)` — acessar id do admin via `admin.0.id`
- `AuthUser { pub id: Uuid, pub is_admin: bool }`
- YAGNI: permissões de file/backup/database/schedule/network são definidas mas não checadas neste plan

---

### Task 1: Compactar migrações em 001_initial.sql

**Files:**
- Delete: `crates/panel/migrations/001_users.sql`
- Delete: `crates/panel/migrations/002_nodes.sql`
- Delete: `crates/panel/migrations/003_servers.sql`
- Delete: `crates/panel/migrations/004_eggs.sql`
- Create: `crates/panel/migrations/001_initial.sql`

**Interfaces:**
- Produces: schema completo com `servers.user_id UUID NOT NULL`, `server_subusers`, todas as tabelas em ordem de dependência

- [ ] **Step 1: Deletar os 4 arquivos de migração antigos**

```bash
rm crates/panel/migrations/001_users.sql \
   crates/panel/migrations/002_nodes.sql \
   crates/panel/migrations/003_servers.sql \
   crates/panel/migrations/004_eggs.sql
```

- [ ] **Step 2: Criar `crates/panel/migrations/001_initial.sql`**

Conteúdo completo do arquivo:

```sql
CREATE TABLE users (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    email         TEXT        NOT NULL UNIQUE,
    password_hash TEXT        NOT NULL,
    is_admin      BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE nodes (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT        NOT NULL UNIQUE,
    grpc_addr  TEXT        NOT NULL,
    token      TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE eggs (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name          TEXT        NOT NULL,
    description   TEXT,
    author        TEXT,
    version       TEXT        NOT NULL DEFAULT '1.0.0',
    features      TEXT[]      NOT NULL DEFAULT '{}',
    file_denylist TEXT[]      NOT NULL DEFAULT '{}',
    docker_images JSONB       NOT NULL DEFAULT '{}',
    start_cmd     TEXT        NOT NULL,
    stop_cmd      TEXT        NOT NULL DEFAULT 'stop',
    startup_done  TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE egg_variables (
    id            UUID    PRIMARY KEY DEFAULT gen_random_uuid(),
    egg_id        UUID    NOT NULL REFERENCES eggs(id) ON DELETE CASCADE,
    name          TEXT    NOT NULL,
    description   TEXT,
    env_variable  TEXT    NOT NULL,
    default_val   TEXT,
    user_viewable BOOLEAN NOT NULL DEFAULT TRUE,
    user_editable BOOLEAN NOT NULL DEFAULT TRUE,
    rules         TEXT,
    field_type    TEXT    NOT NULL DEFAULT 'text'
);

CREATE TABLE egg_install_scripts (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    egg_id     UUID NOT NULL UNIQUE REFERENCES eggs(id) ON DELETE CASCADE,
    container  TEXT NOT NULL,
    entrypoint TEXT NOT NULL DEFAULT 'bash',
    script     TEXT NOT NULL
);

CREATE TABLE egg_config_files (
    id      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    egg_id  UUID NOT NULL REFERENCES eggs(id) ON DELETE CASCADE,
    path    TEXT NOT NULL,
    parser  TEXT NOT NULL CHECK (parser IN ('properties','json','yaml','ini','xml')),
    patches JSONB NOT NULL
);

CREATE TABLE servers (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID        NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    node_id     UUID        NOT NULL REFERENCES nodes(id) ON DELETE RESTRICT,
    egg_id      UUID        REFERENCES eggs(id),
    name        TEXT        NOT NULL UNIQUE,
    image       TEXT        NOT NULL,
    memory_mb   INT         NOT NULL CHECK (memory_mb > 0),
    cpu_percent INT         NOT NULL CHECK (cpu_percent > 0),
    env         TEXT[]      NOT NULL DEFAULT '{}',
    status      TEXT        NOT NULL DEFAULT 'stopped'
                            CHECK (status IN ('installing','running','stopped','error')),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE server_subusers (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    server_id   UUID        NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    user_id     UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    permissions TEXT[]      NOT NULL DEFAULT '{}',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (server_id, user_id)
);
```

- [ ] **Step 3: Verificar que compila**

```bash
cargo build -p oxy-panel 2>&1 | tail -10
```

Expected: erro de compilação em `servers.rs` porque `Server` struct ainda não tem `user_id`. Isso é esperado — será resolvido na Task 3.

- [ ] **Step 4: Commit**

```bash
git add crates/panel/migrations/
git commit -m "feat(panel): compact 4 migrations into single 001_initial.sql"
```

---

### Task 2: permissions.rs — constantes de permissão

**Files:**
- Create: `crates/panel/src/permissions.rs`
- Modify: `crates/panel/src/lib.rs`

**Interfaces:**
- Produces: `crate::permissions::CONTROL_START`, `CONTROL_STOP`, `CONTROL_RESTART`, `CONTROL_CONSOLE`, `USER_CREATE`, `USER_READ`, `USER_UPDATE`, `USER_DELETE`, e demais; `crate::permissions::ALL_PERMISSIONS: &[(&str, &[&str])]`

- [ ] **Step 1: Criar `crates/panel/src/permissions.rs`**

```rust
// Control
pub const CONTROL_CONSOLE: &str = "control.console";
pub const CONTROL_START:   &str = "control.start";
pub const CONTROL_STOP:    &str = "control.stop";
pub const CONTROL_RESTART: &str = "control.restart";

// Users (subuser management)
pub const USER_CREATE: &str = "user.create";
pub const USER_READ:   &str = "user.read";
pub const USER_UPDATE: &str = "user.update";
pub const USER_DELETE: &str = "user.delete";

// Files
pub const FILE_CREATE:       &str = "file.create";
pub const FILE_READ:         &str = "file.read";
pub const FILE_READ_CONTENT: &str = "file.read-content";
pub const FILE_UPDATE:       &str = "file.update";
pub const FILE_DELETE:       &str = "file.delete";
pub const FILE_ARCHIVE:      &str = "file.archive";
pub const FILE_SFTP:         &str = "file.sftp";

// Backups
pub const BACKUP_CREATE:   &str = "backup.create";
pub const BACKUP_READ:     &str = "backup.read";
pub const BACKUP_DELETE:   &str = "backup.delete";
pub const BACKUP_DOWNLOAD: &str = "backup.download";
pub const BACKUP_RESTORE:  &str = "backup.restore";

// Network
pub const NETWORK_READ:   &str = "network.read";
pub const NETWORK_CREATE: &str = "network.create";
pub const NETWORK_UPDATE: &str = "network.update";
pub const NETWORK_DELETE: &str = "network.delete";

// Startup
pub const STARTUP_READ:         &str = "startup.read";
pub const STARTUP_UPDATE:       &str = "startup.update";
pub const STARTUP_DOCKER_IMAGE: &str = "startup.docker-image";

// Databases
pub const DATABASE_CREATE:        &str = "database.create";
pub const DATABASE_READ:          &str = "database.read";
pub const DATABASE_UPDATE:        &str = "database.update";
pub const DATABASE_DELETE:        &str = "database.delete";
pub const DATABASE_VIEW_PASSWORD: &str = "database.view-password";

// Schedules
pub const SCHEDULE_CREATE: &str = "schedule.create";
pub const SCHEDULE_READ:   &str = "schedule.read";
pub const SCHEDULE_UPDATE: &str = "schedule.update";
pub const SCHEDULE_DELETE: &str = "schedule.delete";

// Importer
pub const IMPORTER_ACCESS: &str = "importer.access";

// Settings
pub const SETTINGS_RENAME:     &str = "settings.rename";
pub const SETTINGS_REINSTALL:  &str = "settings.reinstall";
pub const SETTINGS_CHANGE_EGG: &str = "settings.change-egg";

// Activity
pub const ACTIVITY_READ: &str = "activity.read";

pub const ALL_PERMISSIONS: &[(&str, &[&str])] = &[
    ("control",  &[CONTROL_CONSOLE, CONTROL_START, CONTROL_STOP, CONTROL_RESTART]),
    ("user",     &[USER_CREATE, USER_READ, USER_UPDATE, USER_DELETE]),
    ("file",     &[FILE_CREATE, FILE_READ, FILE_READ_CONTENT, FILE_UPDATE, FILE_DELETE, FILE_ARCHIVE, FILE_SFTP]),
    ("backup",   &[BACKUP_CREATE, BACKUP_READ, BACKUP_DELETE, BACKUP_DOWNLOAD, BACKUP_RESTORE]),
    ("network",  &[NETWORK_READ, NETWORK_CREATE, NETWORK_UPDATE, NETWORK_DELETE]),
    ("startup",  &[STARTUP_READ, STARTUP_UPDATE, STARTUP_DOCKER_IMAGE]),
    ("database", &[DATABASE_CREATE, DATABASE_READ, DATABASE_UPDATE, DATABASE_DELETE, DATABASE_VIEW_PASSWORD]),
    ("schedule", &[SCHEDULE_CREATE, SCHEDULE_READ, SCHEDULE_UPDATE, SCHEDULE_DELETE]),
    ("importer", &[IMPORTER_ACCESS]),
    ("settings", &[SETTINGS_RENAME, SETTINGS_REINSTALL, SETTINGS_CHANGE_EGG]),
    ("activity", &[ACTIVITY_READ]),
];

/// Retorna true se a string é uma permissão válida conhecida.
pub fn is_valid_permission(p: &str) -> bool {
    ALL_PERMISSIONS.iter().any(|(_, perms)| perms.contains(&p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_permissions_non_empty() {
        assert!(!ALL_PERMISSIONS.is_empty());
        for (group, perms) in ALL_PERMISSIONS {
            assert!(!perms.is_empty(), "group {} has no permissions", group);
        }
    }

    #[test]
    fn control_start_is_valid() {
        assert!(is_valid_permission(CONTROL_START));
    }

    #[test]
    fn unknown_permission_is_invalid() {
        assert!(!is_valid_permission("hacker.pwn"));
    }

    #[test]
    fn permission_strings_use_dot_convention() {
        for (_, perms) in ALL_PERMISSIONS {
            for p in *perms {
                assert!(p.contains('.'), "permission '{}' missing dot separator", p);
                assert_eq!(*p, p.to_lowercase(), "permission '{}' not lowercase", p);
            }
        }
    }
}
```

- [ ] **Step 2: Expor o módulo em `crates/panel/src/lib.rs`**

Adicione a linha abaixo das outras declarações de módulo (antes de `pub use error::`):

```rust
pub mod permissions;
```

- [ ] **Step 3: Rodar os testes do módulo**

```bash
cargo test -p oxy-panel permissions:: 2>&1 | tail -15
```

Expected: 4 testes passando.

- [ ] **Step 4: Commit**

```bash
git add crates/panel/src/permissions.rs crates/panel/src/lib.rs
git commit -m "feat(panel): permissions module — all Pterodactyl permission constants"
```

---

### Task 3: Server struct user_id + todos os SELECTs + INSERTs de teste

**Files:**
- Modify: `crates/panel/src/servers.rs`

**Interfaces:**
- Consumes: migration `001_initial.sql` com `servers.user_id NOT NULL` (Task 1)
- Produces: `Server { pub user_id: Uuid, ... }` disponível para Tasks 4–6; todos os SELECTs e RETURNING incluem `user_id`

- [ ] **Step 1: Adicionar `pub user_id: Uuid` ao struct `Server`**

Encontre:

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

Substitua por:

```rust
#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct Server {
    pub id:          Uuid,
    pub user_id:     Uuid,
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

- [ ] **Step 2: Atualizar todos os SELECT para incluir `user_id`**

Há 9 queries de SELECT no arquivo. Todas têm o padrão:
```
SELECT id, node_id, name, image, memory_mb, cpu_percent, env, status, created_at
```

Use find-and-replace no arquivo. Encontre:
```
SELECT id, node_id, name, image, memory_mb, cpu_percent, env, status, created_at
```
Substitua por:
```
SELECT id, user_id, node_id, name, image, memory_mb, cpu_percent, env, status, created_at
```

Confirme que foram exatamente 9 substituições (list_servers, get_server, delete_server, start_server, stop_server, provision_server, server_command, server_stats, stream_server_logs).

- [ ] **Step 3: Atualizar o RETURNING do INSERT em `create_server`**

Encontre em `create_server`:
```
RETURNING id, node_id, name, image, memory_mb, cpu_percent, env, status, created_at
```
Substitua por:
```
RETURNING id, user_id, node_id, name, image, memory_mb, cpu_percent, env, status, created_at
```

- [ ] **Step 4: Atualizar os INSERTs diretos nos testes**

Nos testes, há vários `INSERT INTO servers` sem `user_id`. Com `user_id NOT NULL`, eles falharão. Atualize cada um para incluir `user_id`.

Há 4 ocorrências de `INSERT INTO servers` nos testes. Para cada uma, adicione `user_id` e passe `admin_id` (retornado por `seed_admin`).

Padrão atual em cada teste:
```rust
let server_id: Uuid = sqlx::query_scalar::<_, Uuid>(
    "INSERT INTO servers (node_id, name, image, memory_mb, cpu_percent)
     VALUES ($1,$2,$3,$4,$5) RETURNING id",
)
.bind(node_id).bind("srv-name").bind("ubuntu").bind(512).bind(50)
.fetch_one(&pool).await.unwrap();
```

Novo padrão (ajuste o nome do servidor em cada caso):
```rust
let (admin_id, token) = seed_admin(&pool).await;  // seed_admin já retorna (Uuid, String)
// ...
let server_id: Uuid = sqlx::query_scalar::<_, Uuid>(
    "INSERT INTO servers (user_id, node_id, name, image, memory_mb, cpu_percent)
     VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
)
.bind(admin_id).bind(node_id).bind("srv-name").bind("ubuntu").bind(512).bind(50)
.fetch_one(&pool).await.unwrap();
```

Testes afetados:
- `stream_logs_returns_sse_events` — usa `start_log_node`
- `get_stats_proxies_to_node` — usa `start_mock_node`
- `start_server_sets_running_status` — usa `start_mock_node`
- `start_server_sets_error_on_node_failure` — usa `start_fail_node`
- `stop_server_sets_stopped_status` — usa `start_mock_node`

Atenção: `seed_admin` retorna `(Uuid, String)`. Nos testes que antes faziam `let (_, token) = seed_admin(...)`, mude para `let (admin_id, token) = seed_admin(...)`.

- [ ] **Step 5: Verificar que compila**

```bash
cargo build -p oxy-panel 2>&1 | tail -10
```

Expected: compilação bem-sucedida. Se houver erros de "missing field user_id", confirme que o INSERT em `create_server` ainda não inclui `user_id` — isso é resolvido na Task 4.

- [ ] **Step 6: Rodar os testes unitários**

```bash
cargo test -p oxy-panel --lib 2>&1 | grep -E "test result|FAILED|error\[" | head -20
```

Expected: todos os testes unitários (não-DB) passam.

- [ ] **Step 7: Commit**

```bash
git add crates/panel/src/servers.rs
git commit -m "feat(servers): add user_id to Server struct + update all SELECT/RETURNING queries"
```

---

### Task 4: create_server com atribuição de user_id

**Files:**
- Modify: `crates/panel/src/servers.rs`

**Interfaces:**
- Consumes: `Server.user_id: Uuid` (Task 3); `AdminUser(pub AuthUser)` — acessar id via `admin.0.id`
- Produces: `create_server` aceita `user_id` opcional no body; INSERT inclui `user_id`; response inclui `user_id`

- [ ] **Step 1: Escrever o teste que falha**

Adicione no bloco `#[cfg(test)]`, após `create_server_provisions_on_node`:

```rust
#[sqlx::test(migrations = "./migrations")]
async fn create_server_assigns_user_id(pool: sqlx::PgPool) {
    let node_addr = start_mock_node("node-token").await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let (admin_id, token) = seed_admin(&pool).await;
    let node_id = seed_node(&pool, &node_addr).await;

    // criar segundo usuário para testar atribuição explícita
    let owner_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING id",
    )
    .bind("owner@test.com").bind("$argon2id$v=19$m=19456,t=2,p=1$fakehash")
    .fetch_one(&pool).await.unwrap();

    let app = router(make_state(pool));
    let body = serde_json::json!({
        "node_id":     node_id,
        "user_id":     owner_id,
        "name":        "owned-server",
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
    assert_eq!(srv["user_id"].as_str().unwrap(), owner_id.to_string());
}

#[sqlx::test(migrations = "./migrations")]
async fn create_server_defaults_user_id_to_admin(pool: sqlx::PgPool) {
    let node_addr = start_mock_node("node-token").await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let (admin_id, token) = seed_admin(&pool).await;
    let node_id = seed_node(&pool, &node_addr).await;

    let app = router(make_state(pool));
    let body = serde_json::json!({
        "node_id":     node_id,
        "name":        "admin-server",
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
    assert_eq!(srv["user_id"].as_str().unwrap(), admin_id.to_string());
}
```

- [ ] **Step 2: Rodar os testes para confirmar que falham**

```bash
cargo test -p oxy-panel "create_server_assigns_user_id|create_server_defaults_user_id" 2>&1 | tail -10
```

Expected: falha de compilação ou teste falha (INSERT sem user_id → NOT NULL violation).

- [ ] **Step 3: Adicionar `user_id` ao `CreateServerRequest`**

Encontre:
```rust
#[derive(Debug, Deserialize)]
struct CreateServerRequest {
    node_id:     Uuid,
    name:        String,
    image:       String,
    memory_mb:   i32,
    cpu_percent: i32,
    #[serde(default)]
    env:         Vec<String>,
    #[serde(default)]
    egg_id:      Option<Uuid>,
    #[serde(default)]
    egg_vars:    std::collections::HashMap<String, String>,
}
```

Substitua por:

```rust
#[derive(Debug, Deserialize)]
struct CreateServerRequest {
    node_id:     Uuid,
    name:        String,
    image:       String,
    memory_mb:   i32,
    cpu_percent: i32,
    #[serde(default)]
    env:         Vec<String>,
    #[serde(default)]
    egg_id:      Option<Uuid>,
    #[serde(default)]
    egg_vars:    std::collections::HashMap<String, String>,
    #[serde(default)]
    user_id:     Option<Uuid>,
}
```

- [ ] **Step 4: Mudar a assinatura de `create_server` e atualizar o INSERT**

Encontre a assinatura:
```rust
async fn create_server(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<CreateServerRequest>,
) -> Result<(StatusCode, Json<Server>)> {
```

Substitua por:
```rust
async fn create_server(
    State(state): State<AppState>,
    admin: AdminUser,
    Json(body): Json<CreateServerRequest>,
) -> Result<(StatusCode, Json<Server>)> {
```

Depois, logo após as validações de `memory_mb`, `name` etc., adicione:
```rust
    let owner_id = body.user_id.unwrap_or(admin.0.id);
```

Encontre o INSERT:
```rust
    let mut server = sqlx::query_as::<_, Server>(
        "INSERT INTO servers (node_id, name, image, memory_mb, cpu_percent, env, egg_id, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'installing')
         RETURNING id, node_id, name, image, memory_mb, cpu_percent, env, status, created_at",
    )
    .bind(body.node_id)
    .bind(&body.name)
    .bind(&body.image)
    .bind(body.memory_mb)
    .bind(body.cpu_percent)
    .bind(&env)
    .bind(body.egg_id)
    .fetch_one(&state.db)
    .await?;
```

Substitua por:
```rust
    let mut server = sqlx::query_as::<_, Server>(
        "INSERT INTO servers (user_id, node_id, name, image, memory_mb, cpu_percent, env, egg_id, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'installing')
         RETURNING id, user_id, node_id, name, image, memory_mb, cpu_percent, env, status, created_at",
    )
    .bind(owner_id)
    .bind(body.node_id)
    .bind(&body.name)
    .bind(&body.image)
    .bind(body.memory_mb)
    .bind(body.cpu_percent)
    .bind(&env)
    .bind(body.egg_id)
    .fetch_one(&state.db)
    .await?;
```

- [ ] **Step 5: Rodar os testes**

```bash
cargo test -p oxy-panel "create_server" 2>&1 | tail -15
```

Expected: `create_server_assigns_user_id` e `create_server_defaults_user_id_to_admin` passam.

- [ ] **Step 6: Rodar a suite completa para confirmar zero regressões**

```bash
cargo test -p oxy-panel 2>&1 | tail -10
```

- [ ] **Step 7: Commit**

```bash
git add crates/panel/src/servers.rs
git commit -m "feat(servers): create_server assigns user_id — admin specifies owner or defaults to self"
```

---

### Task 5: list_servers filtra por dono + get_server checa ownership

**Files:**
- Modify: `crates/panel/src/servers.rs`

**Interfaces:**
- Consumes: `Server.user_id: Uuid` (Task 3); `AuthUser { id, is_admin }`
- Produces: `list_servers` filtra por user; `get_server` retorna 403 para não-dono não-admin; helper `fetch_server(db, id) -> Result<Server>` para Tasks 6–9

- [ ] **Step 1: Escrever os testes que falham**

Adicione no bloco de testes, após os testes da Task 4:

```rust
#[sqlx::test(migrations = "./migrations")]
async fn list_servers_filters_by_owner(pool: sqlx::PgPool) {
    let node_addr = start_mock_node("node-token").await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let (admin_id, admin_token) = seed_admin(&pool).await;
    let node_id = seed_node(&pool, &node_addr).await;

    // criar segundo usuário
    let other_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING id",
    )
    .bind("other@test.com").bind("$argon2id$v=19$m=19456,t=2,p=1$fakehash")
    .fetch_one(&pool).await.unwrap();
    let other_token = crate::auth::encode_token(other_id, false, "access", SECRET, 900).unwrap();

    // admin cria servidor para si mesmo
    sqlx::query(
        "INSERT INTO servers (user_id, node_id, name, image, memory_mb, cpu_percent)
         VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(admin_id).bind(node_id).bind("admin-srv").bind("ubuntu").bind(512).bind(50)
    .execute(&pool).await.unwrap();

    // admin cria servidor para other
    sqlx::query(
        "INSERT INTO servers (user_id, node_id, name, image, memory_mb, cpu_percent)
         VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(other_id).bind(node_id).bind("other-srv").bind("ubuntu").bind(512).bind(50)
    .execute(&pool).await.unwrap();

    let app = router(make_state(pool));

    // admin vê os dois
    let req = Request::builder()
        .method("GET").uri("/api/servers")
        .header("authorization", format!("Bearer {}", admin_token))
        .body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let list: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 2);

    // other vê só o seu
    let req = Request::builder()
        .method("GET").uri("/api/servers")
        .header("authorization", format!("Bearer {}", other_token))
        .body(Body::empty()).unwrap();
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

    let other_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING id",
    )
    .bind("other2@test.com").bind("$argon2id$v=19$m=19456,t=2,p=1$fakehash")
    .fetch_one(&pool).await.unwrap();
    let other_token = crate::auth::encode_token(other_id, false, "access", SECRET, 900).unwrap();

    let server_id: Uuid = sqlx::query_scalar(
        "INSERT INTO servers (user_id, node_id, name, image, memory_mb, cpu_percent)
         VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
    )
    .bind(admin_id).bind(node_id).bind("forbidden-srv").bind("ubuntu").bind(512).bind(50)
    .fetch_one(&pool).await.unwrap();

    let app = router(make_state(pool));
    let req = Request::builder()
        .method("GET").uri(format!("/api/servers/{}", server_id))
        .header("authorization", format!("Bearer {}", other_token))
        .body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}
```

- [ ] **Step 2: Verificar que os testes falham**

```bash
cargo test -p oxy-panel "list_servers_filters_by_owner|get_server_forbidden_for_non_owner" 2>&1 | tail -10
```

Expected: falha (list retorna 2 para all users; get não checa ownership).

- [ ] **Step 3: Adicionar helper `fetch_server`**

Em `servers.rs`, adicione logo após `get_node_client`:

```rust
async fn fetch_server(db: &sqlx::PgPool, id: Uuid) -> Result<Server> {
    sqlx::query_as::<_, Server>(
        "SELECT id, user_id, node_id, name, image, memory_mb, cpu_percent, env, status, created_at
         FROM servers WHERE id = $1",
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(Into::into)
}
```

- [ ] **Step 4: Atualizar `list_servers` para filtrar por user_id**

Encontre:
```rust
async fn list_servers(
    State(state): State<AppState>,
    _user: AuthUser,
) -> Result<Json<Vec<Server>>> {
    let servers = sqlx::query_as::<_, Server>(
        "SELECT id, user_id, node_id, name, image, memory_mb, cpu_percent, env, status, created_at
         FROM servers ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(servers))
}
```

Substitua por:
```rust
async fn list_servers(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<Server>>> {
    let servers = if user.is_admin {
        sqlx::query_as::<_, Server>(
            "SELECT id, user_id, node_id, name, image, memory_mb, cpu_percent, env, status, created_at
             FROM servers ORDER BY created_at",
        )
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as::<_, Server>(
            "SELECT id, user_id, node_id, name, image, memory_mb, cpu_percent, env, status, created_at
             FROM servers WHERE user_id = $1 ORDER BY created_at",
        )
        .bind(user.id)
        .fetch_all(&state.db)
        .await?
    };
    Ok(Json(servers))
}
```

- [ ] **Step 5: Atualizar `get_server` para checar ownership**

Encontre `get_server` e substitua o corpo inteiro por:

```rust
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
```

- [ ] **Step 6: Rodar os testes**

```bash
cargo test -p oxy-panel "list_servers_filters_by_owner|get_server_forbidden_for_non_owner" 2>&1 | tail -10
```

Expected: ambos passam.

- [ ] **Step 7: Suite completa**

```bash
cargo test -p oxy-panel 2>&1 | tail -10
```

- [ ] **Step 8: Commit**

```bash
git add crates/panel/src/servers.rs
git commit -m "feat(servers): ownership filter on list + 403 on get for non-owner"
```

---

### Task 6: check_server_access helper + access control em start/stop/command/stats/logs

**Files:**
- Modify: `crates/panel/src/servers.rs`

**Interfaces:**
- Consumes: `fetch_server` (Task 5); `crate::permissions::{CONTROL_START, CONTROL_STOP, CONTROL_CONSOLE}` (Task 2)
- Produces: `pub(crate) async fn check_server_access(user: &AuthUser, server: &Server, perm: Option<&str>, db: &PgPool) -> Result<()>` usada pelas Tasks 7 e 9

- [ ] **Step 1: Escrever os testes que falham**

```rust
#[sqlx::test(migrations = "./migrations")]
async fn subuser_with_start_can_start_server(pool: sqlx::PgPool) {
    let node_addr = start_mock_node("node-token").await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let (admin_id, _) = seed_admin(&pool).await;
    let node_id = seed_node(&pool, &node_addr).await;

    let subuser_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING id",
    )
    .bind("sub@test.com").bind("$argon2id$v=19$m=19456,t=2,p=1$fakehash")
    .fetch_one(&pool).await.unwrap();
    let sub_token = crate::auth::encode_token(subuser_id, false, "access", SECRET, 900).unwrap();

    let server_id: Uuid = sqlx::query_scalar(
        "INSERT INTO servers (user_id, node_id, name, image, memory_mb, cpu_percent)
         VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
    )
    .bind(admin_id).bind(node_id).bind("sub-start-srv").bind("ubuntu").bind(512).bind(50)
    .fetch_one(&pool).await.unwrap();

    // adicionar subuser com control.start
    sqlx::query(
        "INSERT INTO server_subusers (server_id, user_id, permissions)
         VALUES ($1, $2, ARRAY['control.start'])",
    )
    .bind(server_id).bind(subuser_id)
    .execute(&pool).await.unwrap();

    let app = router(make_state(pool));
    let req = Request::builder()
        .method("POST").uri(format!("/api/servers/{}/start", server_id))
        .header("authorization", format!("Bearer {}", sub_token))
        .body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

#[sqlx::test(migrations = "./migrations")]
async fn subuser_without_stop_gets_403(pool: sqlx::PgPool) {
    let (admin_id, _) = seed_admin(&pool).await;
    let node_addr = start_mock_node("node-token").await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let node_id = seed_node(&pool, &node_addr).await;

    let subuser_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING id",
    )
    .bind("nosub@test.com").bind("$argon2id$v=19$m=19456,t=2,p=1$fakehash")
    .fetch_one(&pool).await.unwrap();
    let sub_token = crate::auth::encode_token(subuser_id, false, "access", SECRET, 900).unwrap();

    let server_id: Uuid = sqlx::query_scalar(
        "INSERT INTO servers (user_id, node_id, name, image, memory_mb, cpu_percent)
         VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
    )
    .bind(admin_id).bind(node_id).bind("nosub-srv").bind("ubuntu").bind(512).bind(50)
    .fetch_one(&pool).await.unwrap();

    // subuser com control.start mas NÃO control.stop
    sqlx::query(
        "INSERT INTO server_subusers (server_id, user_id, permissions)
         VALUES ($1, $2, ARRAY['control.start'])",
    )
    .bind(server_id).bind(subuser_id)
    .execute(&pool).await.unwrap();

    let app = router(make_state(pool));
    let req = Request::builder()
        .method("POST").uri(format!("/api/servers/{}/stop", server_id))
        .header("authorization", format!("Bearer {}", sub_token))
        .body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn stranger_gets_403_on_start(pool: sqlx::PgPool) {
    let (admin_id, _) = seed_admin(&pool).await;
    let node_addr = start_mock_node("node-token").await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let node_id = seed_node(&pool, &node_addr).await;

    let stranger_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING id",
    )
    .bind("stranger@test.com").bind("$argon2id$v=19$m=19456,t=2,p=1$fakehash")
    .fetch_one(&pool).await.unwrap();
    let stranger_token = crate::auth::encode_token(stranger_id, false, "access", SECRET, 900).unwrap();

    let server_id: Uuid = sqlx::query_scalar(
        "INSERT INTO servers (user_id, node_id, name, image, memory_mb, cpu_percent)
         VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
    )
    .bind(admin_id).bind(node_id).bind("stranger-srv").bind("ubuntu").bind(512).bind(50)
    .fetch_one(&pool).await.unwrap();

    let app = router(make_state(pool));
    let req = Request::builder()
        .method("POST").uri(format!("/api/servers/{}/start", server_id))
        .header("authorization", format!("Bearer {}", stranger_token))
        .body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}
```

- [ ] **Step 2: Rodar para confirmar que falham**

```bash
cargo test -p oxy-panel "subuser_with_start|subuser_without_stop|stranger_gets_403" 2>&1 | tail -10
```

- [ ] **Step 3: Adicionar `check_server_access` em `servers.rs`**

Adicione logo após `fetch_server`:

```rust
pub(crate) async fn check_server_access(
    user: &AuthUser,
    server: &Server,
    perm: Option<&str>,
    db: &sqlx::PgPool,
) -> Result<()> {
    if user.is_admin || server.user_id == user.id {
        return Ok(());
    }
    let perms: Vec<String> = sqlx::query_scalar(
        "SELECT unnest(permissions) FROM server_subusers
         WHERE server_id = $1 AND user_id = $2",
    )
    .bind(server.id)
    .bind(user.id)
    .fetch_all(db)
    .await?;
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
```

- [ ] **Step 4: Atualizar `start_server` com check de acesso**

Adicione import no topo do arquivo (junto aos outros use de permissions):
```rust
use crate::permissions::{CONTROL_CONSOLE, CONTROL_START, CONTROL_STOP};
```

Substitua o corpo de `start_server` por:
```rust
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
                .bind(server.id).execute(&state.db).await?;
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            let _ = sqlx::query("UPDATE servers SET status = 'error' WHERE id = $1")
                .bind(server.id).execute(&state.db).await;
            Err(e)
        }
    }
}
```

- [ ] **Step 5: Atualizar `stop_server` com check de acesso**

```rust
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
        .bind(server.id).execute(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

- [ ] **Step 6: Atualizar `server_command` com check de acesso**

```rust
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
    client.send_command(&server.id.to_string(), &content).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

- [ ] **Step 7: Atualizar `server_stats` com check de acesso (qualquer subuser)**

```rust
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
```

- [ ] **Step 8: Atualizar `stream_server_logs` com check de acesso**

```rust
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
```

- [ ] **Step 9: Simplificar os outros handlers que já usam fetch_server**

Os handlers `delete_server`, `provision_server` (admin-only, usam `AdminUser`) ainda têm o SELECT inline. Substitua em cada um por:

```rust
let server = fetch_server(&state.db, id).await?;
```

(Remove a repetição do SELECT inline — o helper faz exatamente o mesmo.)

- [ ] **Step 10: Rodar os testes**

```bash
cargo test -p oxy-panel "subuser_with_start|subuser_without_stop|stranger_gets_403" 2>&1 | tail -15
```

Expected: todos os 3 passam.

- [ ] **Step 11: Suite completa**

```bash
cargo test -p oxy-panel 2>&1 | tail -10
```

- [ ] **Step 12: Commit**

```bash
git add crates/panel/src/servers.rs
git commit -m "feat(servers): check_server_access + permission enforcement on start/stop/command/stats/logs"
```

---

### Task 7: restart_server endpoint

**Files:**
- Modify: `crates/panel/src/servers.rs`

**Interfaces:**
- Consumes: `fetch_server`, `check_server_access`, `CONTROL_RESTART` (Tasks 2, 5, 6)
- Produces: `POST /api/servers/:id/restart` — stop best-effort + start; atualiza status

- [ ] **Step 1: Escrever o teste que falha**

```rust
#[sqlx::test(migrations = "./migrations")]
async fn restart_server_transitions_to_running(pool: sqlx::PgPool) {
    let node_addr = start_mock_node("node-token").await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let (admin_id, token) = seed_admin(&pool).await;
    let node_id = seed_node(&pool, &node_addr).await;

    let server_id: Uuid = sqlx::query_scalar(
        "INSERT INTO servers (user_id, node_id, name, image, memory_mb, cpu_percent)
         VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
    )
    .bind(admin_id).bind(node_id).bind("restart-srv").bind("ubuntu").bind(512).bind(50)
    .fetch_one(&pool).await.unwrap();

    let app = router(make_state(pool.clone()));
    let req = Request::builder()
        .method("POST").uri(format!("/api/servers/{}/restart", server_id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let status: String = sqlx::query_scalar("SELECT status FROM servers WHERE id = $1")
        .bind(server_id).fetch_one(&pool).await.unwrap();
    assert_eq!(status, "running");
}

#[sqlx::test(migrations = "./migrations")]
async fn restart_forbidden_for_non_owner(pool: sqlx::PgPool) {
    let (admin_id, _) = seed_admin(&pool).await;
    let node_addr = start_mock_node("node-token").await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let node_id = seed_node(&pool, &node_addr).await;

    let other_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING id",
    )
    .bind("rother@test.com").bind("$argon2id$v=19$m=19456,t=2,p=1$fakehash")
    .fetch_one(&pool).await.unwrap();
    let other_token = crate::auth::encode_token(other_id, false, "access", SECRET, 900).unwrap();

    let server_id: Uuid = sqlx::query_scalar(
        "INSERT INTO servers (user_id, node_id, name, image, memory_mb, cpu_percent)
         VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
    )
    .bind(admin_id).bind(node_id).bind("restart-forbidden").bind("ubuntu").bind(512).bind(50)
    .fetch_one(&pool).await.unwrap();

    let app = router(make_state(pool));
    let req = Request::builder()
        .method("POST").uri(format!("/api/servers/{}/restart", server_id))
        .header("authorization", format!("Bearer {}", other_token))
        .body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}
```

- [ ] **Step 2: Verificar que falham (rota não existe)**

```bash
cargo test -p oxy-panel "restart_server_transitions|restart_forbidden" 2>&1 | tail -10
```

- [ ] **Step 3: Adicionar import `CONTROL_RESTART` e handler**

Atualize o import das permissões:
```rust
use crate::permissions::{CONTROL_CONSOLE, CONTROL_RESTART, CONTROL_START, CONTROL_STOP};
```

Adicione o handler após `stop_server`:

```rust
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
                .bind(server.id).execute(&state.db).await?;
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            let _ = sqlx::query("UPDATE servers SET status = 'error' WHERE id = $1")
                .bind(server.id).execute(&state.db).await;
            Err(e)
        }
    }
}
```

- [ ] **Step 4: Registrar a rota em `servers_router`**

Encontre:
```rust
.route("/:id/stop",      post(stop_server))
```

Adicione logo abaixo:
```rust
.route("/:id/restart",   post(restart_server))
```

- [ ] **Step 5: Rodar os testes**

```bash
cargo test -p oxy-panel "restart_server_transitions|restart_forbidden" 2>&1 | tail -10
```

- [ ] **Step 6: Suite completa**

```bash
cargo test -p oxy-panel 2>&1 | tail -10
```

- [ ] **Step 7: Commit**

```bash
git add crates/panel/src/servers.rs
git commit -m "feat(servers): POST /:id/restart — stop+start lifecycle with control.restart permission"
```

---

### Task 8: GET /api/me endpoint

**Files:**
- Modify: `crates/panel/src/users.rs`
- Modify: `crates/panel/src/lib.rs`

**Interfaces:**
- Consumes: `AuthUser { id, is_admin }`
- Produces: `GET /api/me → { id, email, is_admin }` — usado pelo frontend para saber quem está logado

- [ ] **Step 1: Escrever o teste que falha**

Adicione no bloco de testes de `users.rs`:

```rust
#[sqlx::test(migrations = "./migrations")]
async fn me_returns_current_user(pool: sqlx::PgPool) {
    let (_, token) = seed_admin(&pool).await;
    let app = router(make_state(pool));
    let req = Request::builder()
        .method("GET").uri("/api/me")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let me: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(me["email"], "a@t.com");
    assert_eq!(me["is_admin"], true);
    assert!(me["id"].as_str().is_some());
}

#[sqlx::test(migrations = "./migrations")]
async fn me_requires_auth(pool: sqlx::PgPool) {
    let app = router(make_state(pool));
    let req = Request::builder()
        .method("GET").uri("/api/me")
        .body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
```

- [ ] **Step 2: Verificar que falham (rota não existe)**

```bash
cargo test -p oxy-panel "me_returns_current_user|me_requires_auth" 2>&1 | tail -10
```

- [ ] **Step 3: Adicionar struct `MeResponse` e handler `me` em `users.rs`**

Adicione após as structs existentes:

```rust
#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct MeResponse {
    pub id:       Uuid,
    pub email:    String,
    pub is_admin: bool,
}
```

Adicione o handler antes de `users_router`:

```rust
pub async fn me(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<MeResponse>> {
    let row = sqlx::query_as::<_, MeResponse>(
        "SELECT id, email, is_admin FROM users WHERE id = $1",
    )
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(row))
}
```

- [ ] **Step 4: Registrar a rota em `lib.rs`**

Em `pub fn router`, adicione:
```rust
.route("/api/me", get(users::me))
```

O import de `get` já existe implicitamente via axum. O arquivo `lib.rs` ficará:

```rust
use axum::routing::get;

pub fn router(state: AppState) -> axum::Router {
    axum::Router::new()
        .route("/api/me", get(users::me))
        .nest("/auth",        auth::auth_router())
        .nest("/api/users",   users::users_router())
        .nest("/api/nodes",   nodes::nodes_router())
        .nest("/api/servers", servers::servers_router())
        .nest("/api/eggs",    eggs::eggs_router())
        .with_state(state)
}
```

- [ ] **Step 5: Rodar os testes**

```bash
cargo test -p oxy-panel "me_returns_current_user|me_requires_auth" 2>&1 | tail -10
```

- [ ] **Step 6: Suite completa**

```bash
cargo test -p oxy-panel 2>&1 | tail -10
```

- [ ] **Step 7: Commit**

```bash
git add crates/panel/src/users.rs crates/panel/src/lib.rs
git commit -m "feat(users): GET /api/me — returns current user id, email, is_admin"
```

---

### Task 9: subusers.rs — GET e POST (listar e adicionar)

**Files:**
- Create: `crates/panel/src/subusers.rs`
- Modify: `crates/panel/src/lib.rs`
- Modify: `crates/panel/src/servers.rs`

**Interfaces:**
- Consumes: `crate::permissions::{USER_CREATE, USER_READ, is_valid_permission}`; `AppState`; `AuthUser`
- Produces: `pub fn subusers_router() -> Router<AppState>`; `ServerSubuser { id, server_id, user_id, permissions, created_at }`

- [ ] **Step 1: Escrever os testes que falham**

Em `crates/panel/src/subusers.rs` (arquivo novo que você vai criar, com o bloco `#[cfg(test)]` incluso):

Adicione no final do novo arquivo:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        auth::{encode_token, hash_password},
        router, AppState,
    };
    use axum::{body::Body, http::{Request, StatusCode}};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use uuid::Uuid;

    const SECRET: &str = "test-secret-at-least-32-chars-long!!";

    fn make_state(pool: sqlx::PgPool) -> AppState {
        AppState { db: pool, jwt_secret: SECRET.to_string() }
    }

    async fn seed_admin(pool: &sqlx::PgPool) -> (Uuid, String) {
        let id = Uuid::new_v4();
        let hash = hash_password("pass").unwrap();
        sqlx::query("INSERT INTO users (id, email, password_hash, is_admin) VALUES ($1,$2,$3,$4)")
            .bind(id).bind("a@t.com").bind(&hash).bind(true)
            .execute(pool).await.unwrap();
        let token = encode_token(id, true, "access", SECRET, 900).unwrap();
        (id, token)
    }

    async fn seed_user(pool: &sqlx::PgPool, email: &str) -> (Uuid, String) {
        let id = Uuid::new_v4();
        let hash = hash_password("pass").unwrap();
        sqlx::query("INSERT INTO users (id, email, password_hash) VALUES ($1,$2,$3)")
            .bind(id).bind(email).bind(&hash)
            .execute(pool).await.unwrap();
        let token = encode_token(id, false, "access", SECRET, 900).unwrap();
        (id, token)
    }

    async fn seed_node(pool: &sqlx::PgPool) -> Uuid {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO nodes (name, grpc_addr, token) VALUES ($1,$2,$3) RETURNING id",
        )
        .bind("n").bind("http://127.0.0.1:1").bind("tok")
        .fetch_one(pool).await.unwrap()
    }

    async fn seed_server(pool: &sqlx::PgPool, user_id: Uuid, node_id: Uuid, name: &str) -> Uuid {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO servers (user_id, node_id, name, image, memory_mb, cpu_percent)
             VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
        )
        .bind(user_id).bind(node_id).bind(name).bind("ubuntu").bind(512).bind(50)
        .fetch_one(pool).await.unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn owner_can_add_subuser(pool: sqlx::PgPool) {
        let (admin_id, admin_token) = seed_admin(&pool).await;
        let (sub_id, _) = seed_user(&pool, "sub@t.com").await;
        let node_id = seed_node(&pool).await;
        let server_id = seed_server(&pool, admin_id, node_id, "sub-srv").await;

        let app = router(make_state(pool));
        let body = serde_json::json!({
            "user_id":     sub_id,
            "permissions": ["control.start", "control.stop"],
        });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/servers/{}/subusers", server_id))
            .header("authorization", format!("Bearer {}", admin_token))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let su: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(su["user_id"].as_str().unwrap(), sub_id.to_string());
        let perms = su["permissions"].as_array().unwrap();
        assert!(perms.iter().any(|p| p == "control.start"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn non_owner_cannot_add_subuser(pool: sqlx::PgPool) {
        let (admin_id, _) = seed_admin(&pool).await;
        let (other_id, other_token) = seed_user(&pool, "other@t.com").await;
        let node_id = seed_node(&pool).await;
        let server_id = seed_server(&pool, admin_id, node_id, "perm-srv").await;

        let app = router(make_state(pool));
        let body = serde_json::json!({ "user_id": other_id, "permissions": [] });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/servers/{}/subusers", server_id))
            .header("authorization", format!("Bearer {}", other_token))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn invalid_permission_rejected(pool: sqlx::PgPool) {
        let (admin_id, admin_token) = seed_admin(&pool).await;
        let (sub_id, _) = seed_user(&pool, "inv@t.com").await;
        let node_id = seed_node(&pool).await;
        let server_id = seed_server(&pool, admin_id, node_id, "inv-srv").await;

        let app = router(make_state(pool));
        let body = serde_json::json!({
            "user_id":     sub_id,
            "permissions": ["hacker.pwn"],
        });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/servers/{}/subusers", server_id))
            .header("authorization", format!("Bearer {}", admin_token))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn owner_can_list_subusers(pool: sqlx::PgPool) {
        let (admin_id, admin_token) = seed_admin(&pool).await;
        let (sub_id, _) = seed_user(&pool, "list@t.com").await;
        let node_id = seed_node(&pool).await;
        let server_id = seed_server(&pool, admin_id, node_id, "list-srv").await;

        sqlx::query(
            "INSERT INTO server_subusers (server_id, user_id, permissions)
             VALUES ($1,$2,ARRAY['control.start'])",
        )
        .bind(server_id).bind(sub_id)
        .execute(&pool).await.unwrap();

        let app = router(make_state(pool));
        let req = Request::builder()
            .method("GET")
            .uri(format!("/api/servers/{}/subusers", server_id))
            .header("authorization", format!("Bearer {}", admin_token))
            .body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let list: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(list.as_array().unwrap().len(), 1);
    }
}
```

- [ ] **Step 2: Verificar que falham (arquivo não existe)**

```bash
cargo build -p oxy-panel 2>&1 | tail -5
```

Expected: erro de compilação (módulo não encontrado).

- [ ] **Step 3: Criar `crates/panel/src/subusers.rs` com structs e handlers GET + POST**

```rust
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
    auth::AuthUser,
    error::{PanelError, Result},
    permissions::{is_valid_permission, USER_CREATE, USER_READ},
    AppState,
};

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct ServerSubuser {
    pub id:          Uuid,
    pub server_id:   Uuid,
    pub user_id:     Uuid,
    pub permissions: Vec<String>,
    pub created_at:  DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct SubuserBody {
    pub user_id:     Uuid,
    pub permissions: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSubuserBody {
    pub permissions: Vec<String>,
}

/// Verifica se o usuário é admin, dono do servidor ou subuser com a permissão dada.
async fn check_access(
    user: &AuthUser,
    server_id: Uuid,
    perm: &str,
    db: &sqlx::PgPool,
) -> Result<()> {
    if user.is_admin {
        return Ok(());
    }
    let owner_id: Option<Uuid> = sqlx::query_scalar("SELECT user_id FROM servers WHERE id = $1")
        .bind(server_id)
        .fetch_optional(db)
        .await?;
    let owner_id = owner_id.ok_or_else(|| PanelError::NotFound(format!("server {}", server_id)))?;
    if owner_id == user.id {
        return Ok(());
    }
    let perms: Vec<String> = sqlx::query_scalar(
        "SELECT unnest(permissions) FROM server_subusers
         WHERE server_id = $1 AND user_id = $2",
    )
    .bind(server_id)
    .bind(user.id)
    .fetch_all(db)
    .await?;
    if perms.iter().any(|p| p == perm) {
        Ok(())
    } else {
        Err(PanelError::Forbidden)
    }
}

pub async fn list_subusers(
    State(state): State<AppState>,
    user: AuthUser,
    Path(server_id): Path<Uuid>,
) -> Result<Json<Vec<ServerSubuser>>> {
    check_access(&user, server_id, USER_READ, &state.db).await?;
    let subusers = sqlx::query_as::<_, ServerSubuser>(
        "SELECT id, server_id, user_id, permissions, created_at
         FROM server_subusers WHERE server_id = $1 ORDER BY created_at",
    )
    .bind(server_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(subusers))
}

pub async fn create_subuser(
    State(state): State<AppState>,
    user: AuthUser,
    Path(server_id): Path<Uuid>,
    Json(body): Json<SubuserBody>,
) -> Result<(StatusCode, Json<ServerSubuser>)> {
    check_access(&user, server_id, USER_CREATE, &state.db).await?;
    for p in &body.permissions {
        if !is_valid_permission(p) {
            return Err(PanelError::Validation(format!("unknown permission: {}", p)));
        }
    }
    let subuser = sqlx::query_as::<_, ServerSubuser>(
        "INSERT INTO server_subusers (server_id, user_id, permissions)
         VALUES ($1, $2, $3)
         RETURNING id, server_id, user_id, permissions, created_at",
    )
    .bind(server_id)
    .bind(body.user_id)
    .bind(&body.permissions)
    .fetch_one(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(subuser)))
}

pub fn subusers_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_subusers).post(create_subuser))
}
```

- [ ] **Step 4: Declarar o módulo em `lib.rs` e registrar as rotas**

Em `lib.rs`, adicione `mod subusers;` junto aos outros módulos.

Em `servers::servers_router()` (em `servers.rs`), adicione as rotas de subusers. Adicione `use crate::subusers;` no início de `servers.rs`, e na função `servers_router`:

```rust
pub fn servers_router() -> Router<AppState> {
    Router::new()
        .route("/",              get(list_servers).post(create_server))
        .route("/:id",           get(get_server).delete(delete_server))
        .route("/:id/start",     post(start_server))
        .route("/:id/stop",      post(stop_server))
        .route("/:id/restart",   post(restart_server))
        .route("/:id/provision", post(provision_server))
        .route("/:id/command",   post(server_command))
        .route("/:id/stats",     get(server_stats))
        .route("/:id/logs",      get(stream_server_logs))
        .route("/:id/subusers",  get(subusers::list_subusers).post(subusers::create_subuser))
}
```

- [ ] **Step 5: Rodar os testes**

```bash
cargo test -p oxy-panel "owner_can_add_subuser|non_owner_cannot_add_subuser|invalid_permission_rejected|owner_can_list_subusers" 2>&1 | tail -15
```

Expected: todos os 4 passam.

- [ ] **Step 6: Suite completa**

```bash
cargo test -p oxy-panel 2>&1 | tail -10
```

- [ ] **Step 7: Commit**

```bash
git add crates/panel/src/subusers.rs crates/panel/src/lib.rs crates/panel/src/servers.rs
git commit -m "feat(subusers): GET + POST /api/servers/:id/subusers with permission validation"
```

---

### Task 10: subusers PATCH + DELETE (atualizar e remover)

**Files:**
- Modify: `crates/panel/src/subusers.rs`
- Modify: `crates/panel/src/servers.rs`

**Interfaces:**
- Consumes: `check_access`, `UpdateSubuserBody`, `USER_UPDATE`, `USER_DELETE` de `subusers.rs` (Task 9)
- Produces: `PATCH /api/servers/:id/subusers/:uid` e `DELETE /api/servers/:id/subusers/:uid`

- [ ] **Step 1: Escrever os testes que falham**

Adicione no bloco `#[cfg(test)]` de `subusers.rs`:

```rust
#[sqlx::test(migrations = "./migrations")]
async fn owner_can_update_subuser_permissions(pool: sqlx::PgPool) {
    let (admin_id, admin_token) = seed_admin(&pool).await;
    let (sub_id, _) = seed_user(&pool, "upd@t.com").await;
    let node_id = seed_node(&pool).await;
    let server_id = seed_server(&pool, admin_id, node_id, "upd-srv").await;

    let subuser_id: Uuid = sqlx::query_scalar(
        "INSERT INTO server_subusers (server_id, user_id, permissions)
         VALUES ($1,$2,ARRAY['control.start']) RETURNING id",
    )
    .bind(server_id).bind(sub_id)
    .fetch_one(&pool).await.unwrap();

    let app = router(make_state(pool));
    let body = serde_json::json!({ "permissions": ["control.start", "control.stop"] });
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/servers/{}/subusers/{}", server_id, subuser_id))
        .header("authorization", format!("Bearer {}", admin_token))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let su: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let perms = su["permissions"].as_array().unwrap();
    assert!(perms.iter().any(|p| p == "control.stop"));
}

#[sqlx::test(migrations = "./migrations")]
async fn owner_can_delete_subuser(pool: sqlx::PgPool) {
    let (admin_id, admin_token) = seed_admin(&pool).await;
    let (sub_id, _) = seed_user(&pool, "del@t.com").await;
    let node_id = seed_node(&pool).await;
    let server_id = seed_server(&pool, admin_id, node_id, "del-srv").await;

    let subuser_id: Uuid = sqlx::query_scalar(
        "INSERT INTO server_subusers (server_id, user_id, permissions)
         VALUES ($1,$2,ARRAY[]::text[]) RETURNING id",
    )
    .bind(server_id).bind(sub_id)
    .fetch_one(&pool).await.unwrap();

    let app = router(make_state(pool.clone()));
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/servers/{}/subusers/{}", server_id, subuser_id))
        .header("authorization", format!("Bearer {}", admin_token))
        .body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM server_subusers WHERE id = $1",
    )
    .bind(subuser_id)
    .fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn non_owner_cannot_delete_subuser(pool: sqlx::PgPool) {
    let (admin_id, _) = seed_admin(&pool).await;
    let (other_id, other_token) = seed_user(&pool, "del2@t.com").await;
    let node_id = seed_node(&pool).await;
    let server_id = seed_server(&pool, admin_id, node_id, "del2-srv").await;

    let subuser_id: Uuid = sqlx::query_scalar(
        "INSERT INTO server_subusers (server_id, user_id, permissions)
         VALUES ($1,$2,ARRAY[]::text[]) RETURNING id",
    )
    .bind(server_id).bind(other_id)
    .fetch_one(&pool).await.unwrap();

    let app = router(make_state(pool));
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/servers/{}/subusers/{}", server_id, subuser_id))
        .header("authorization", format!("Bearer {}", other_token))
        .body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}
```

- [ ] **Step 2: Verificar que falham (rotas não existem)**

```bash
cargo test -p oxy-panel "owner_can_update_subuser|owner_can_delete_subuser|non_owner_cannot_delete" 2>&1 | tail -10
```

- [ ] **Step 3: Adicionar handlers `update_subuser` e `delete_subuser` em `subusers.rs`**

Adicione após `create_subuser`:

```rust
pub async fn update_subuser(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, subuser_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdateSubuserBody>,
) -> Result<Json<ServerSubuser>> {
    check_access(&user, server_id, USER_UPDATE, &state.db).await?;
    for p in &body.permissions {
        if !is_valid_permission(p) {
            return Err(PanelError::Validation(format!("unknown permission: {}", p)));
        }
    }
    let subuser = sqlx::query_as::<_, ServerSubuser>(
        "UPDATE server_subusers SET permissions = $1
         WHERE id = $2 AND server_id = $3
         RETURNING id, server_id, user_id, permissions, created_at",
    )
    .bind(&body.permissions)
    .bind(subuser_id)
    .bind(server_id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(subuser))
}

pub async fn delete_subuser(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, subuser_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode> {
    check_access(&user, server_id, USER_DELETE, &state.db).await?;
    sqlx::query("DELETE FROM server_subusers WHERE id = $1 AND server_id = $2")
        .bind(subuser_id)
        .bind(server_id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
```

Atualize também `subusers_router` para incluir as novas permissões no import:

```rust
use crate::permissions::{is_valid_permission, USER_CREATE, USER_DELETE, USER_READ, USER_UPDATE};
```

- [ ] **Step 4: Registrar as rotas PATCH e DELETE em `servers.rs`**

Encontre em `servers_router`:
```rust
.route("/:id/subusers",  get(subusers::list_subusers).post(subusers::create_subuser))
```

Adicione logo abaixo:
```rust
.route("/:id/subusers/:uid", axum::routing::patch(subusers::update_subuser).delete(subusers::delete_subuser))
```

- [ ] **Step 5: Rodar os testes**

```bash
cargo test -p oxy-panel "owner_can_update_subuser|owner_can_delete_subuser|non_owner_cannot_delete" 2>&1 | tail -15
```

Expected: todos os 3 passam.

- [ ] **Step 6: Suite completa**

```bash
cargo test -p oxy-panel 2>&1 | tail -10
```

Expected: todos os testes unitários passam; testes DB skipped sem DATABASE_URL.

- [ ] **Step 7: Commit**

```bash
git add crates/panel/src/subusers.rs crates/panel/src/servers.rs
git commit -m "feat(subusers): PATCH + DELETE /api/servers/:id/subusers/:uid"
```
