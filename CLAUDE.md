# Oxydactylus — Contexto para Claude

## O que é

Painel de gerenciamento de servidores de jogos em Rust, substituto do Pterodactyl. Arquitetura cliente/servidor: daemon rodando em cada máquina (`oxy-node`) + painel central (`oxy-panel`) com API REST + frontend Leptos (Plan 8, ainda não implementado).

## Workspace (crates)

```
crates/
  core/      — tipos compartilhados: Config, OxyError, proto definitions (gRPC)
  node/      — daemon do nó: Docker via Bollard, gRPC server (NodeService)
  panel/     — backend REST: axum, sqlx/PostgreSQL, JWT auth
  cli/       — entrypoint binário: lê config.toml e inicia panel ou node
```

## Stack técnica

| Camada | Tech |
|--------|------|
| Linguagem | Rust 2021, workspace resolver = "2" |
| Web framework | axum 0.7 |
| Banco de dados | PostgreSQL via sqlx 0.8 |
| Auth | JWT (jsonwebtoken 9) + argon2 |
| gRPC | tonic 0.12 + prost 0.13 |
| Docker | bollard |
| Frontend (Plan 8) | Leptos CSR + Trunk |

## API REST (panel)

```
POST   /auth/login                          — { email, password } → { access_token, refresh_token }
POST   /auth/refresh                        — { refresh_token } → { access_token }
GET    /api/me                              — usuário autenticado atual
GET    /api/users                           — lista usuários (admin)
POST   /api/users                           — cria usuário (admin)
DELETE /api/users/:id                       — remove usuário (admin)
GET    /api/nodes                           — lista nodes (admin)
POST   /api/nodes                           — cria node (admin)
DELETE /api/nodes/:id                       — remove node (admin)
GET    /api/servers                         — lista servidores (admin: todos; user: próprios)
POST   /api/servers                         — cria servidor (admin) { node_id, user_id, name, image, memory_mb, cpu_percent, egg_id?, egg_vars? }
GET    /api/servers/:id                     — detalhe do servidor
DELETE /api/servers/:id                     — remove servidor (admin)
POST   /api/servers/:id/start              — inicia servidor
POST   /api/servers/:id/stop               — para servidor
POST   /api/servers/:id/restart            — reinicia servidor
POST   /api/servers/:id/provision          — re-provisiona (admin)
POST   /api/servers/:id/command            — envia comando ao console { content }
GET    /api/servers/:id/stats              — { memory_bytes, cpu_percent, rx_bytes, tx_bytes }
GET    /api/servers/:id/logs?follow=bool   — SSE stream de logs
GET    /api/servers/:id/subusers           — lista subusers
POST   /api/servers/:id/subusers           — adiciona subuser { user_id, permissions[] }
PATCH  /api/servers/:id/subusers/:uid      — atualiza permissões { permissions[] }
DELETE /api/servers/:id/subusers/:uid      — remove subuser
GET    /api/eggs                           — lista eggs
POST   /api/eggs                           — cria egg
GET    /api/eggs/:id                       — detalhe egg
DELETE /api/eggs/:id                       — remove egg
GET    /api/eggs/:id/variables             — variáveis do egg
POST   /api/eggs/:id/variables             — cria variável
DELETE /api/eggs/:id/variables/:vid        — remove variável
GET    /api/eggs/:id/install-script        — script de instalação
PUT    /api/eggs/:id/install-script        — upsert script
GET    /api/eggs/:id/config-files          — config files do egg
POST   /api/eggs/:id/config-files          — adiciona config file
DELETE /api/eggs/:id/config-files/:cid     — remove config file
POST   /api/eggs/import                    — importa PTDL v2 (JSON)
GET    /api/eggs/:id/export                — exporta egg como .toml
```

## Autenticação

- `AuthUser` extractor: qualquer JWT válido
- `AdminUser` extractor: JWT com `is_admin = true`, retorna 403 caso contrário
- JWT no header: `Authorization: Bearer <token>`

## Status de Servidor

Valores: `'installing'` | `'running'` | `'stopped'` | `'error'`

Transições:
- `create_server` → INSERT `'installing'` → provision → UPDATE `'stopped'`
- `start_server` → gRPC ok → `'running'` | gRPC fail → `'error'`
- `stop_server` → gRPC ok → `'stopped'` | gRPC fail → sem mudança
- `restart_server` → stop (best-effort) → start → `'running'` | fail → `'error'`

## Sistema de Permissões (subusers)

Tabela `server_subusers`: `server_id + user_id + permissions TEXT[]`

Grupos de permissão (constantes em `crates/panel/src/permissions.rs`):
- `control.*` → console, start, stop, restart
- `user.*` → create, read, update, delete (gerenciar outros subusers)
- `file.*` → create, read, read-content, update, delete, archive, sftp *(futuro)*
- `backup.*` → create, read, delete, download, restore *(futuro)*
- `network.*` → read, create, update, delete *(futuro)*
- `startup.*` → read, update, docker-image
- `database.*` → create, read, update, delete, view-password *(futuro)*
- `schedule.*` → create, read, update, delete *(futuro)*
- `importer.*` → access *(futuro)*
- `settings.*` → rename, reinstall, change-egg *(futuro)*
- `activity.*` → read

Acesso: admin > dono do servidor > subuser com permissão específica.

## Schema do Banco

Migração única: `crates/panel/migrations/001_initial.sql`

Tabelas: `users`, `nodes`, `eggs`, `egg_variables`, `egg_install_scripts`, `egg_config_files`, `servers`, `server_subusers`

```
users
  id, email, password_hash, is_admin, created_at

nodes
  id, name, grpc_addr, token, created_at

eggs
  id, name, description, author, version, features[], file_denylist[],
  docker_images (JSONB), start_cmd, stop_cmd, startup_done, created_at, updated_at

egg_variables
  id, egg_id→eggs, name, description, env_variable, default_val,
  user_viewable, user_editable, rules, field_type

egg_install_scripts
  id, egg_id→eggs (UNIQUE), container, entrypoint, script

egg_config_files
  id, egg_id→eggs, path, parser, patches (JSONB)

servers
  id, user_id→users (NOT NULL), node_id→nodes, egg_id→eggs?,
  name (UNIQUE), image, memory_mb, cpu_percent,
  env TEXT[], status, created_at

server_subusers
  id, server_id→servers, user_id→users,
  permissions TEXT[], created_at
  UNIQUE(server_id, user_id)
```

## gRPC (NodeService — proto em crates/core/proto/)

Métodos: `ProvisionServer`, `StartServer`, `StopServer`, `DeleteServer`, `SendCommand`, `GetStats`, `StreamLogs`

Auth: `Authorization: Bearer <node_token>` no metadata gRPC.

## Padrões de Código

- `crate::error::Result<T>` = alias de um param; `std::result::Result<T, E>` nos dois-param (tonic, trait impls)
- Testes: `#[sqlx::test(migrations = "./migrations")]` — requer `DATABASE_URL`
- Mocks gRPC nos testes: implementam `NodeService` inline com `AcceptAllNode`, `FailStartNode`, etc.
- `pool.clone()` quando o teste precisa acessar o DB após passar o pool ao app state

## Planos Completados

| Plan | Descrição | Commits |
|------|-----------|---------|
| 1 | Foundation: workspace, core, node stub, panel stub, CLI | a13952f..929c3cf |
| 2 | Node daemon: Docker/Bollard + gRPC completo | 6fef53f..d81e130 |
| 3 | Panel backend: auth, CRUD users/nodes/servers | d81e130..468dff3 |
| 4 | Eggs: CRUD, variáveis, PTDL import, regras | 741dd9c..9b5360b |
| 5 | Log streaming: SSE `GET /servers/:id/logs` | 9b5360b..a14dd91 |
| 6 | Server status tracking: installing/running/stopped/error | 3bd1e7f..3510d50 |

## Próximos Planos

| Plan | Descrição |
|------|-----------|
| 7 | Backend: compactar migrações + ownership + subusers com permissões |
| 8 | Frontend Leptos CSR: admin area + client area |

## Convenções

- Sem comentários no código (exceto WHY não-óbvio)
- YAGNI rigoroso
- Commits frequentes por tarefa
- Subagent-Driven Development para implementação (skill superpowers:subagent-driven-development)
- Responder em português (usuário é brasileiro)
