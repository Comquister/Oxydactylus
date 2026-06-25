# Server Ownership + Subusers — Design Spec

**Data:** 2026-06-25
**Status:** Aprovado

## Objetivo

Adicionar propriedade de servidores (cada servidor pertence a um usuário) e um sistema completo de subusers com permissões granulares compatíveis com Pterodactyl. Isso permite que usuários comuns acessem os próprios servidores via painel e que doem acesso parcial a outras pessoas.

## Escopo

- Compactar as 4 migrações (001–004) em um único `001_initial.sql` limpo
- Adicionar `server_subusers` com `permissions TEXT[]`
- Definir todas as constantes de permissão em `src/permissions.rs`
- Expor e enforcar ownership em `servers.rs`
- Novo arquivo `src/subusers.rs` com CRUD de subusers
- Novo endpoint `GET /api/me`
- Novo endpoint `POST /api/servers/:id/restart`

## Migração Consolidada

### Arquivo único: `crates/panel/migrations/001_initial.sql`
Substitui 001_users, 002_nodes, 003_servers, 004_eggs.

Ordem de criação (respeitando FKs):

```sql
-- 1. users
CREATE TABLE users (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    email         TEXT        NOT NULL UNIQUE,
    password_hash TEXT        NOT NULL,
    is_admin      BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 2. nodes
CREATE TABLE nodes (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT        NOT NULL UNIQUE,
    grpc_addr  TEXT        NOT NULL,
    token      TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 3. eggs (antes de servers por FK)
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

-- 4. servers (depende de users, nodes, eggs)
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

-- 5. server_subusers (depende de servers e users)
CREATE TABLE server_subusers (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    server_id   UUID        NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    user_id     UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    permissions TEXT[]      NOT NULL DEFAULT '{}',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (server_id, user_id)
);
```

**Removido:** coluna `env_vars JSONB` (existia em 004 mas nunca usada no código).
**Renomeado:** `owner_id` → `user_id` com `NOT NULL`.

## Sistema de Permissões

### Arquivo: `crates/panel/src/permissions.rs`

Constantes para todos os grupos. Convenção: `GRUPO_ACAO`.

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

// Files (futuros — definidos agora para o frontend)
pub const FILE_CREATE:       &str = "file.create";
pub const FILE_READ:         &str = "file.read";
pub const FILE_READ_CONTENT: &str = "file.read-content";
pub const FILE_UPDATE:       &str = "file.update";
pub const FILE_DELETE:       &str = "file.delete";
pub const FILE_ARCHIVE:      &str = "file.archive";
pub const FILE_SFTP:         &str = "file.sftp";

// Backups (futuros)
pub const BACKUP_CREATE:   &str = "backup.create";
pub const BACKUP_READ:     &str = "backup.read";
pub const BACKUP_DELETE:   &str = "backup.delete";
pub const BACKUP_DOWNLOAD: &str = "backup.download";
pub const BACKUP_RESTORE:  &str = "backup.restore";

// Network (futuros)
pub const NETWORK_READ:   &str = "network.read";
pub const NETWORK_CREATE: &str = "network.create";
pub const NETWORK_UPDATE: &str = "network.update";
pub const NETWORK_DELETE: &str = "network.delete";

// Startup
pub const STARTUP_READ:         &str = "startup.read";
pub const STARTUP_UPDATE:       &str = "startup.update";
pub const STARTUP_DOCKER_IMAGE: &str = "startup.docker-image";

// Databases (futuros)
pub const DATABASE_CREATE:        &str = "database.create";
pub const DATABASE_READ:          &str = "database.read";
pub const DATABASE_UPDATE:        &str = "database.update";
pub const DATABASE_DELETE:        &str = "database.delete";
pub const DATABASE_VIEW_PASSWORD: &str = "database.view-password";

// Schedules (futuros)
pub const SCHEDULE_CREATE: &str = "schedule.create";
pub const SCHEDULE_READ:   &str = "schedule.read";
pub const SCHEDULE_UPDATE: &str = "schedule.update";
pub const SCHEDULE_DELETE: &str = "schedule.delete";

// Importer
pub const IMPORTER_ACCESS: &str = "importer.access";

// Settings
pub const SETTINGS_RENAME:      &str = "settings.rename";
pub const SETTINGS_REINSTALL:   &str = "settings.reinstall";
pub const SETTINGS_CHANGE_EGG:  &str = "settings.change-egg";

// Activity
pub const ACTIVITY_READ: &str = "activity.read";

/// Todos os grupos e suas permissões (para o frontend listar)
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
```

## Access Control

### Helper em `servers.rs`

```rust
/// Verifica se o usuário pode executar uma ação num servidor.
/// Admin e dono sempre podem. Subuser precisa ter a permissão específica.
/// Permissão None = qualquer subuser (ex: stats).
async fn check_server_access(
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

`PanelError::Forbidden` → HTTP 403. Adicionar essa variante ao enum em `error.rs`.

### Permissões por endpoint (Plan 7)

| Endpoint | Permissão exigida |
|---|---|
| `GET /api/servers` | — (filtra por user_id) |
| `GET /api/servers/:id` | `startup.read` (subuser) |
| `POST /api/servers/:id/start` | `control.start` |
| `POST /api/servers/:id/stop` | `control.stop` |
| `POST /api/servers/:id/restart` | `control.restart` |
| `POST /api/servers/:id/command` | `control.console` |
| `GET /api/servers/:id/logs` | `control.console` |
| `GET /api/servers/:id/stats` | `None` (qualquer subuser) |
| Subuser CRUD | `user.*` |

Endpoints admin-only (AdminUser extractor): `create_server`, `delete_server`, `provision_server`.

## Novo Endpoint: `GET /api/me`

Em `users.rs`:

```rust
async fn me(State(state): State<AppState>, user: AuthUser) -> Result<Json<MeResponse>> {
    let row = sqlx::query_as::<_, MeResponse>(
        "SELECT id, email, is_admin FROM users WHERE id = $1",
    )
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(row))
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct MeResponse {
    pub id:       Uuid,
    pub email:    String,
    pub is_admin: bool,
}
```

Rota: `GET /api/me` (sem prefixo de usuário).

## Novo Endpoint: `POST /api/servers/:id/restart`

Em `servers.rs`:

```rust
async fn restart_server(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let server = fetch_server(&state.db, id).await?;
    check_server_access(&user, &server, Some(CONTROL_RESTART), &state.db).await?;
    let mut client = get_node_client(&state, server.node_id).await?;
    // stop (ignora erro se já parado)
    let _ = client.stop(&server.id.to_string(), 10).await;
    // start
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

## Subusers (`crates/panel/src/subusers.rs`)

```rust
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
```

Endpoints:

```
GET    /api/servers/:id/subusers       → user.read (ou dono/admin)
POST   /api/servers/:id/subusers       → user.create
PATCH  /api/servers/:id/subusers/:uid  → user.update
DELETE /api/servers/:id/subusers/:uid  → user.delete
```

Validação: permissões enviadas devem ser strings conhecidas (verificar contra `ALL_PERMISSIONS`). Subuser não pode ter `user.create/update/delete` se o solicitante não tiver essas permissões.

## Mudanças em `servers.rs`

- `Server` struct: adicionar `pub user_id: Uuid`
- Todos os SELECT: adicionar `user_id` nas colunas
- `create_server`: body ganha `user_id: Option<Uuid>` (admin usa para assignar; omitido = próprio admin)
- Todos os handlers que antes usavam `_user: AuthUser` sem checagem: agora chamam `check_server_access`
- Extrair helper `fetch_server(db, id) -> Result<Server>` para evitar repetição

## Mudanças em `error.rs`

Adicionar:

```rust
#[error("forbidden")]
Forbidden,
```

Mapear para HTTP 403 no `IntoResponse`.

## Mudanças em `lib.rs`

```rust
.route("/api/me", get(users::me))
.nest("/api/servers/:id/subusers", subusers::subusers_router())
```

## Testes

- `migration_schema_is_valid` — `#[sqlx::test]` que só conecta (valida que a migration roda sem erro)
- `create_server_requires_user_id` — admin cria servidor com user_id; response inclui user_id
- `list_servers_filters_by_owner` — user vê só seus servidores
- `non_owner_gets_403` — usuário sem relação com o servidor recebe 403
- `subuser_with_start_can_start` — subuser com `control.start` consegue iniciar
- `subuser_without_stop_cannot_stop` — subuser sem `control.stop` recebe 403
- `me_returns_current_user` — `GET /api/me` retorna dados do usuário autenticado
- `restart_transitions_to_running` — restart chama stop+start e atualiza status

## Restrições Globais

- Rust edition 2021, workspace resolver = "2"
- Substituir 001–004 por um único `001_initial.sql` (nenhum dado de produção existe)
- `PanelError::Forbidden` → HTTP 403
- Permissões: strings lowercase com ponto (`control.start`), validadas contra `ALL_PERMISSIONS`
- YAGNI: permissões de file/backup/database/schedule são definidas mas não checadas (sem endpoints)
- Testes: `#[sqlx::test(migrations = "./migrations")]`
