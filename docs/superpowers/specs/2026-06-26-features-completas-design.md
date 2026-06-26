# Plan 9: Features Completas Pterodactyl

**Data:** 2026-06-26
**Status:** Aprovado

## Objetivo

Implementar as features restantes do Pterodactyl no Oxydactylus: gerenciador de arquivos com SFTP, databases por servidor, agendamentos, backups locais, alocações de rede, edição de variáveis de startup, settings de servidor e log de atividade. Também migrar o painel para suportar SQLite, MySQL e PostgreSQL via `sqlx AnyPool`.

## Referência

Schema baseado nas migrations do Pterodactyl em `/opt/conquister-pteropanel/database/migrations/`.

---

## 1. Multi-Database (sqlx AnyPool)

### Motivação

O painel atualmente só suporta PostgreSQL. Para facilitar self-hosting, o banco é selecionado via `DATABASE_URL` em `config.toml` (`sqlite://`, `mysql://`, `postgres://`).

### Mudanças

- Substituir `PgPool` por `AnyPool` em todo `crates/panel/`
- Chamar `sqlx::any::install_default_drivers()` no startup antes de criar o pool
- Reescrever todas as migrations em SQL portátil:

| Tipo Postgres | Tipo Portátil |
|---|---|
| `UUID` | `TEXT` |
| `TEXT[]` / `TEXT ARRAY` | `TEXT` (JSON array serializado) |
| `JSONB` | `TEXT` (JSON serializado) |
| `TIMESTAMPTZ` | `TEXT` (ISO8601) |
| `BOOLEAN` | `BOOLEAN` (sqlx normaliza para cada banco) |

- Compactar as migrations atuais em uma única `001_initial.sql` portátil
- Deserializar campos JSON via `serde_json` no código Rust

---

## 2. Network / Allocations

### Schema

```sql
CREATE TABLE allocations (
    id TEXT PRIMARY KEY,
    node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    ip TEXT NOT NULL,
    ip_alias TEXT,              -- domain ou hostname (alias visível ao cliente)
    port INTEGER NOT NULL,
    server_id TEXT REFERENCES servers(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    UNIQUE(node_id, ip, port)
);
```

Alterações em `servers`:
- Adicionar `allocation_id TEXT REFERENCES allocations(id)` — alocação primária do servidor
- Adicionar `database_limit INTEGER NOT NULL DEFAULT 0`
- Adicionar `backup_limit INTEGER NOT NULL DEFAULT 0`

### Comportamento

- **Admin:** cria alocações por node em batch; aceita ranges de porta (ex: `"3000-3010"`)
- **Cliente:** vê alocações do servidor, adiciona novas até o limite definido no servidor, não pode remover a alocação primária
- **gRPC `StartServer`:** passa port bindings ao Docker a partir das alocações atribuídas ao servidor

### API

**Admin (nodes):**
- `GET /api/nodes/:id/allocations` — lista todas as alocações do node
- `POST /api/nodes/:id/allocations` — cria em batch `{ ip, ip_alias?, ports: [3000, "3001-3010"] }`
- `DELETE /api/nodes/:id/allocations/:aid` — remove (apenas se não atribuída)

**Servidor (network):**
- `GET /api/servers/:id/network` — lista alocações atribuídas
- `POST /api/servers/:id/network` — atribui alocação disponível do mesmo node
- `DELETE /api/servers/:id/network/:aid` — desatribui (não pode ser a primária)
- `POST /api/servers/:id/network/:aid/make-primary` — troca a alocação primária

---

## 3. Files + SFTP

### gRPC — Novos métodos em `NodeService`

```protobuf
rpc ListFiles(ListFilesRequest) returns (ListFilesResponse);
rpc GetFileContents(FilePathRequest) returns (FileContentsResponse);
rpc WriteFileContents(WriteFileRequest) returns (google.protobuf.Empty);
rpc CreateDirectory(FilePathRequest) returns (google.protobuf.Empty);
rpc DeleteFiles(DeleteFilesRequest) returns (google.protobuf.Empty);
rpc RenameFile(RenameFileRequest) returns (google.protobuf.Empty);
rpc CopyFile(CopyFileRequest) returns (google.protobuf.Empty);
rpc CompressFiles(CompressFilesRequest) returns (CompressResponse);
rpc DecompressFile(FilePathRequest) returns (google.protobuf.Empty);
rpc DownloadFile(FilePathRequest) returns (stream FileChunk);
rpc UploadFile(stream UploadChunk) returns (google.protobuf.Empty);
```

### Implementação no Node

Acesso ao filesystem do container via Docker `exec` (comandos `ls`, `cat`, `mkdir`, `rm`, `mv`, `cp`, `tar`) ou acesso direto ao path do volume montado via Bollard.

### SFTP

Servidor SFTP embutido no daemon do node usando a crate `russh`:
- Porta configurável por node (`sftp_port`, default `2022`)
- Autenticação: username = UUID do servidor, password = token do painel **ou** SSH key cadastrada pelo usuário
- Traduz operações SFTP para o filesystem do container

### Schema

```sql
CREATE TABLE user_ssh_keys (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    fingerprint TEXT NOT NULL,  -- SHA256 da chave pública
    public_key TEXT NOT NULL,
    created_at TEXT NOT NULL
);
```

Alteração em `nodes`: adicionar `sftp_port INTEGER NOT NULL DEFAULT 2022`.

### API (Panel)

**Arquivos:**
- `GET  /api/servers/:id/files?directory=/`
- `GET  /api/servers/:id/files/contents?file=/path/to/file`
- `POST /api/servers/:id/files/contents?file=/path/to/file`
- `POST /api/servers/:id/files/create-directory` `{ root, name }`
- `POST /api/servers/:id/files/delete` `{ root, files: ["file1", "dir/"] }`
- `PUT  /api/servers/:id/files/rename` `{ root, files: [{from, to}] }`
- `POST /api/servers/:id/files/copy` `{ location }`
- `POST /api/servers/:id/files/compress` `{ root, files }` → `{ name }` (arquivo gerado)
- `POST /api/servers/:id/files/decompress` `{ root, file }`
- `GET  /api/servers/:id/files/download?file=/path` — stream proxiado do node
- `POST /api/servers/:id/files/upload?directory=/` — multipart upload

**SSH Keys (conta do usuário):**
- `GET    /api/account/ssh-keys`
- `POST   /api/account/ssh-keys` `{ name, public_key }`
- `DELETE /api/account/ssh-keys/:fingerprint`

---

## 4. Databases

### Schema

```sql
CREATE TABLE database_hosts (
    id TEXT PRIMARY KEY,
    node_id TEXT REFERENCES nodes(id) ON DELETE SET NULL,   -- host pode não estar ligado a um node
    name TEXT NOT NULL,
    host TEXT NOT NULL,
    port INTEGER NOT NULL DEFAULT 3306,
    username TEXT NOT NULL,
    password TEXT NOT NULL,         -- armazenado criptografado (AES-256-GCM via chave em config)
    max_databases INTEGER NOT NULL DEFAULT 0,   -- 0 = ilimitado
    created_at TEXT NOT NULL
);

CREATE TABLE server_databases (
    id TEXT PRIMARY KEY,
    server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    host_id TEXT NOT NULL REFERENCES database_hosts(id),
    database_name TEXT NOT NULL,    -- nome real no host MySQL
    username TEXT NOT NULL,
    remote TEXT NOT NULL DEFAULT '%',   -- host permitido para conexão
    password TEXT NOT NULL,         -- armazenado criptografado
    created_at TEXT NOT NULL,
    UNIQUE(host_id, database_name)
);
```

Alteração em `servers`: `database_limit INTEGER NOT NULL DEFAULT 0` (0 = ilimitado).

### Comportamento

O panel conecta **diretamente** ao host MySQL (sem gRPC) para gerenciar databases:
- Criar: `CREATE DATABASE`, `CREATE USER`, `GRANT ALL`, `FLUSH PRIVILEGES`
- Deletar: `DROP DATABASE`, `DROP USER`
- Rotate password: `ALTER USER ... IDENTIFIED BY`
- Pool MySQL por host, cacheado no `AppState`

### API

**Admin:**
- `GET    /api/database-hosts`
- `POST   /api/database-hosts` `{ node_id?, name, host, port, username, password, max_databases? }`
- `GET    /api/database-hosts/:id`
- `DELETE /api/database-hosts/:id`
- `POST   /api/database-hosts/:id/test` — testa conexão

**Servidor:**
- `GET    /api/servers/:id/databases`
- `POST   /api/servers/:id/databases` `{ host_id, database?, remote? }`
- `DELETE /api/servers/:id/databases/:dbid`
- `POST   /api/servers/:id/databases/:dbid/rotate-password`

---

## 5. Schedules

### Schema

```sql
CREATE TABLE schedules (
    id TEXT PRIMARY KEY,
    server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    cron_minute TEXT NOT NULL DEFAULT '*',
    cron_hour TEXT NOT NULL DEFAULT '*',
    cron_day_of_month TEXT NOT NULL DEFAULT '*',
    cron_month TEXT NOT NULL DEFAULT '*',
    cron_day_of_week TEXT NOT NULL DEFAULT '*',
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    is_processing BOOLEAN NOT NULL DEFAULT FALSE,
    only_when_online BOOLEAN NOT NULL DEFAULT FALSE,    -- só executa se servidor estiver running
    last_run_at TEXT,
    next_run_at TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE schedule_tasks (
    id TEXT PRIMARY KEY,
    schedule_id TEXT NOT NULL REFERENCES schedules(id) ON DELETE CASCADE,
    sequence_id INTEGER NOT NULL,                   -- ordem de execução
    action TEXT NOT NULL,                           -- 'power' | 'command' | 'backup'
    payload TEXT NOT NULL,                          -- JSON:
                                                    --   power:   { "action": "start"|"stop"|"restart"|"kill" }
                                                    --   command: { "command": "say hello" }
                                                    --   backup:  {}
    time_offset INTEGER NOT NULL DEFAULT 0,         -- segundos após o disparo do schedule
    is_queued BOOLEAN NOT NULL DEFAULT FALSE,
    continue_on_failure BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TEXT NOT NULL
);
```

### Scheduler

Módulo `crates/panel/src/scheduler.rs`, iniciado como `tokio::spawn` no startup:
- Tick a cada 60 segundos
- Usa crate `cron` para parsear expressões e calcular `next_run_at`
- Ao disparar: marca `is_processing = true`, executa tasks em sequência respeitando `time_offset` via `tokio::time::sleep`
- Após concluir: atualiza `last_run_at`, calcula novo `next_run_at`, marca `is_processing = false`
- Se `only_when_online = true`: checa status do servidor antes de executar; pula se não estiver `running`

### API

- `GET    /api/servers/:id/schedules`
- `POST   /api/servers/:id/schedules` `{ name, cron_minute, cron_hour, cron_day_of_month, cron_month, cron_day_of_week, is_active, only_when_online }`
- `GET    /api/servers/:id/schedules/:sid`
- `PUT    /api/servers/:id/schedules/:sid`
- `DELETE /api/servers/:id/schedules/:sid`
- `POST   /api/servers/:id/schedules/:sid/execute` — trigger manual imediato
- `GET    /api/servers/:id/schedules/:sid/tasks`
- `POST   /api/servers/:id/schedules/:sid/tasks` `{ action, payload, time_offset, sequence_id, continue_on_failure }`
- `PUT    /api/servers/:id/schedules/:sid/tasks/:tid`
- `DELETE /api/servers/:id/schedules/:sid/tasks/:tid`

---

## 6. Backups

### Schema

```sql
CREATE TABLE backups (
    id TEXT PRIMARY KEY,
    server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    uuid TEXT NOT NULL UNIQUE,          -- identifica o arquivo físico no node
    name TEXT NOT NULL,
    ignored_files TEXT NOT NULL DEFAULT '[]',   -- JSON array de paths a ignorar
    driver TEXT NOT NULL DEFAULT 'local',       -- 'local' por agora; abstrato para futuro
    sha256_hash TEXT,
    bytes INTEGER NOT NULL DEFAULT 0,
    is_successful BOOLEAN NOT NULL DEFAULT FALSE,
    is_locked BOOLEAN NOT NULL DEFAULT FALSE,   -- impede deleção acidental
    completed_at TEXT,
    created_at TEXT NOT NULL
);
```

Alteração em `servers`: `backup_limit INTEGER NOT NULL DEFAULT 0` (0 = ilimitado).

### BackupDriver

Trait em `crates/panel/src/backup/driver.rs`:

```rust
#[async_trait]
pub trait BackupDriver: Send + Sync {
    async fn initiate(&self, server_id: Uuid, backup: &Backup) -> Result<()>;
    async fn delete(&self, server_id: Uuid, backup: &Backup) -> Result<()>;
    async fn restore(&self, server_id: Uuid, backup: &Backup) -> Result<()>;
    async fn download_url(&self, backup: &Backup, base_url: &str) -> Result<String>;
}
```

`LocalBackupDriver`: delega para gRPC. Arquivos salvos em `/var/lib/oxy/backups/{uuid}.tar.gz` no node.

### gRPC — Novos métodos

```protobuf
rpc CreateBackup(CreateBackupRequest) returns (CreateBackupResponse);  // retorna sha256 + bytes
rpc DeleteBackup(BackupRequest) returns (google.protobuf.Empty);
rpc RestoreBackup(BackupRequest) returns (google.protobuf.Empty);
rpc DownloadBackup(BackupRequest) returns (stream FileChunk);
```

### API

- `GET    /api/servers/:id/backups`
- `POST   /api/servers/:id/backups` `{ name, ignored_files? }`
- `GET    /api/servers/:id/backups/:bid`
- `DELETE /api/servers/:id/backups/:bid` — recusado se `is_locked = true`
- `GET    /api/servers/:id/backups/:bid/download` — stream proxiado do node
- `POST   /api/servers/:id/backups/:bid/restore`
- `POST   /api/servers/:id/backups/:bid/lock` — toggle `is_locked`

---

## 7. Startup

Sem novas tabelas. Usa `eggs`, `egg_variables` e `servers.env` (JSON após migração).

### Comportamento

`GET /api/servers/:id/startup` retorna:
- Lista de `egg_variables` do egg do servidor, com `current_value` sobreposto do `servers.env`
- Flags `user_viewable` e `user_editable` para cada variável
- Regras de validação (`rules`) para cada variável
- Opções de docker image disponíveis (do egg) e image atual do servidor

`PUT /api/servers/:id/startup` `{ env: { "VAR": "value" } }`:
- Valida que cada variável enviada é `user_editable`
- Aplica as `rules` do egg_variable antes de salvar
- Atualiza apenas as variáveis enviadas no `servers.env`

`PUT /api/servers/:id/startup/docker-image` `{ docker_image }` — somente admin.

---

## 8. Settings

### API

- `PATCH /api/servers/:id` `{ name }` — renomear; requer owner ou permissão `settings.rename`
- `POST  /api/servers/:id/settings/reinstall` — re-executa install script; sets `status = 'installing'` → chama `ProvisionServer` gRPC → sets `status = 'stopped'`; requer owner ou `settings.reinstall`
- `POST  /api/servers/:id/settings/change-egg` `{ egg_id, image?, reset_env? }` — somente admin
- `POST  /api/servers/:id/settings/suspend` — somente admin; sets `status = 'suspended'` (novo status)
- `POST  /api/servers/:id/settings/unsuspend` — somente admin; sets `status = 'stopped'`

Novo valor de status: `'suspended'` — servidor suspenso não pode ser iniciado por ninguém além de admin.

---

## 9. Activity Log

### Schema

```sql
CREATE TABLE activity_logs (
    id TEXT PRIMARY KEY,
    batch_id TEXT,                              -- agrupa ações relacionadas (ex: reinstall)
    server_id TEXT REFERENCES servers(id) ON DELETE CASCADE,
    user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    event TEXT NOT NULL,                        -- ex: 'server:power.start'
    properties TEXT NOT NULL DEFAULT '{}',      -- JSON com detalhes do evento
    ip TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX activity_logs_server_id_idx ON activity_logs(server_id);
CREATE INDEX activity_logs_event_idx ON activity_logs(event);
```

### Helper

`log_activity(pool, entry: ActivityEntry)` chamado diretamente nos handlers — sem middleware; cada handler que deve ser auditado chama explicitamente.

### Eventos

| Prefixo | Eventos |
|---|---|
| `server:power` | `.start`, `.stop`, `.restart`, `.kill` |
| `server:console` | `.command` |
| `server:files` | `.read`, `.write`, `.delete`, `.rename`, `.copy`, `.compress`, `.decompress`, `.upload`, `.download` |
| `server:databases` | `.create`, `.delete`, `.rotate-password` |
| `server:backups` | `.create`, `.delete`, `.restore`, `.download`, `.lock` |
| `server:schedules` | `.create`, `.run`, `.delete` |
| `server:network` | `.allocation-add`, `.allocation-remove` |
| `server:startup` | `.update`, `.docker-image` |
| `server:settings` | `.rename`, `.reinstall`, `.change-egg`, `.suspend`, `.unsuspend` |
| `server:subuser` | `.create`, `.update`, `.delete` |
| `user` | `.create`, `.delete` |
| `node` | `.create`, `.delete` |

### API

- `GET /api/servers/:id/activity?page=1&per_page=50` — atividade do servidor (paginada)
- `GET /api/activity?page=1&per_page=50` — toda atividade (admin)
- `GET /api/account/activity?page=1&per_page=50` — atividade do usuário atual

---

## Estrutura de Arquivos Novos

```
crates/
  core/proto/
    node.proto           — novos métodos Files + Backups
  node/src/
    sftp.rs              — servidor SFTP com russh
    files.rs             — implementação gRPC de arquivos
    backups.rs           — implementação gRPC de backups
  panel/src/
    allocations.rs       — handlers de alocações
    files.rs             — handlers de arquivo (proxy para gRPC)
    ssh_keys.rs          — handlers de SSH keys do usuário
    database_hosts.rs    — handlers de database hosts (admin)
    server_databases.rs  — handlers de databases por servidor
    schedules.rs         — handlers de schedules e tasks
    scheduler.rs         — background scheduler (tokio::spawn)
    backups.rs           — handlers de backups
    startup.rs           — handlers de startup vars
    settings.rs          — handlers de settings
    activity.rs          — handlers de activity log + helper log_activity()
    backup/
      mod.rs
      driver.rs          — trait BackupDriver
      local.rs           — LocalBackupDriver
  panel/migrations/
    001_initial.sql      — schema único e portátil (compacta tudo)
```

---

## Config (`config.toml`) — Campos Novos

```toml
[panel]
app_key = "base64-encoded-32-bytes"   # chave AES-256-GCM para criptografar senhas de database hosts
```

## Dependências Novas

| Crate | Motivo |
|---|---|
| `sqlx` com feature `any` | multi-database |
| `russh` + `russh-sftp` | servidor SFTP no node |
| `cron` | parse de expressões cron no scheduler |
| `aes-gcm` | criptografia de senhas de database hosts |

---

## Ordem de Implementação

1. **Multi-DB + schema portátil** — base para todo o resto
2. **Allocations** — necessário para StartServer correto
3. **Files** (gRPC + API) — sem SFTP primeiro
4. **SFTP** — adiciona ao node após files funcionar
5. **Database Hosts + Server Databases**
6. **Schedules + Scheduler**
7. **Backups** (gRPC + API + BackupDriver)
8. **Startup**
9. **Settings**
10. **Activity Log** — adiciona `log_activity()` em todos os handlers anteriores
