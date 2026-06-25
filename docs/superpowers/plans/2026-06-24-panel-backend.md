# Panel Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `oxy-panel` HTTP API — JWT auth, PostgreSQL persistence for users/nodes/servers, a `ProvisionServer` gRPC RPC on the node, and a gRPC client proxy so the panel can manage containers.

**Architecture:** Axum 0.7 HTTP server with SQLx 0.8 (PostgreSQL, runtime-tokio-rustls). Auth uses stateless JWTs (access 15 min, refresh 7 days) via `jsonwebtoken 9`; passwords hashed with `argon2 0.5`. The panel stores `users`, `nodes`, and `servers` metadata; for lifecycle operations it creates a `NodeClient` per request and proxies calls to the appropriate node daemon via tonic gRPC. Container identity on the node is the server's UUID (`server.id.to_string()`).

**Tech Stack:** `axum 0.7`, `sqlx 0.8` (postgres + runtime-tokio-rustls + uuid + chrono + migrate), `jsonwebtoken 9`, `argon2 0.5`, `tonic 0.12` (client), `chrono 0.4`, `serde_json 1`

## Global Constraints

- All DB queries use runtime API `sqlx::query_as::<_, T>()` — no `sqlx::query!` macro, no `DATABASE_URL` required at compile time for production code
- `#[sqlx::test]` used for DB-touching tests — requires `DATABASE_URL` env var pointing to a running Postgres instance at test time
- `PanelError` implements `axum::response::IntoResponse` — all handlers return `Result<_, PanelError>`
- Passwords hashed with `argon2::Argon2::default()` and `password_hash::SaltString::generate` — never plain or reversible
- Argon2 hashing runs in `tokio::task::spawn_blocking` — it is CPU-bound, not async
- JWT secret comes from `AppState.jwt_secret` (set from `PanelConfig.jwt_secret`) — never hardcoded
- Container name on the node = `server.id.to_string()` (UUID string) — not `server.name`
- `NodeClient` is created per-request (not cached in AppState) — YAGNI until profiling shows it matters
- `NodeClient` methods take `&mut self` (tonic 0.12 generated clients require `&mut self`)
- No `unwrap()` or `expect()` in non-test code
- `env` column in `servers` table stores `Vec<String>` as `TEXT[]` in Postgres; sqlx decodes via `Vec<String>` with the `postgres` feature

---

### Task 1: Deps + PanelError + DB pool + migrations + PanelConfig.jwt_secret

**Files:**
- Modify: `Cargo.toml` (workspace root — add 7 new workspace deps)
- Modify: `crates/core/src/config.rs` (add `jwt_secret` to `PanelConfig`)
- Modify: `config.example.toml` (add `jwt_secret` field under `[panel]`)
- Modify: `crates/panel/Cargo.toml`
- Create: `crates/panel/src/error.rs`
- Create: `crates/panel/src/db.rs`
- Create: `crates/panel/migrations/001_users.sql`
- Create: `crates/panel/migrations/002_nodes.sql`
- Create: `crates/panel/migrations/003_servers.sql`
- Modify: `crates/panel/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub enum PanelError` with variants: `NotFound(String)`, `Unauthorized(String)`, `Forbidden`, `Conflict(String)`, `Validation(String)`, `Db(String)`, `Node(String)`, `Internal(String)`
  - `pub type Result<T> = std::result::Result<T, PanelError>`
  - `impl From<sqlx::Error> for PanelError`
  - `impl From<tonic::Status> for PanelError`
  - `impl IntoResponse for PanelError`
  - `pub async fn create_pool(database_url: &str) -> Result<PgPool>`
  - `pub async fn run_migrations(pool: &PgPool) -> Result<()>`
  - `pub struct AppState { pub db: PgPool, pub jwt_secret: String }`
  - `pub fn router(state: AppState) -> axum::Router` (stub, expanded in later tasks)
  - `PanelConfig.jwt_secret: String`

- [ ] **Step 1: Add workspace dependencies**

In `Cargo.toml`, add to the `[workspace.dependencies]` section:
```toml
axum         = { version = "0.7", features = ["macros"] }
sqlx         = { version = "0.8", features = ["postgres", "runtime-tokio-rustls", "uuid", "chrono", "migrate"] }
jsonwebtoken = "9"
argon2       = "0.5"
chrono       = { version = "0.4", features = ["serde"] }
serde_json   = "1"
tower        = { version = "0.4", features = ["util"] }
```

- [ ] **Step 2: Replace crates/panel/Cargo.toml**

```toml
[package]
name    = "oxy-panel"
version = "0.1.0"
edition = "2021"

[dependencies]
oxy-core     = { path = "../core" }
tokio        = { workspace = true }
tonic        = { workspace = true }
tracing      = { workspace = true }
thiserror    = { workspace = true }
serde        = { workspace = true }
uuid         = { workspace = true }
axum         = { workspace = true }
sqlx         = { workspace = true }
jsonwebtoken = { workspace = true }
argon2       = { workspace = true }
chrono       = { workspace = true }
serde_json   = { workspace = true }

[dev-dependencies]
sqlx           = { workspace = true, features = ["test"] }
tower          = { workspace = true }
http-body-util = "0.1"
```

- [ ] **Step 3: Add jwt_secret to PanelConfig**

In `crates/core/src/config.rs`, update `PanelConfig`:
```rust
#[derive(Debug, Clone, Deserialize)]
pub struct PanelConfig {
    pub http_listen:  String,
    pub database_url: String,
    pub jwt_secret:   String,
}
```

Also update the `parses_panel_role` and `parses_both_role` tests in that file to include `jwt_secret`:
```toml
# in the test TOML strings, add under [panel]:
jwt_secret   = "test-jwt-secret"
```

- [ ] **Step 4: Update config.example.toml**

In every `[panel]` block in `config.example.toml`, add:
```toml
jwt_secret   = "change-me-to-a-random-64-char-secret"
```

- [ ] **Step 5: Write PanelError tests**

Create `crates/panel/src/error.rs` with tests first:
```rust
use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum PanelError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("forbidden")]
    Forbidden,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("validation: {0}")]
    Validation(String),
    #[error("database error: {0}")]
    Db(String),
    #[error("node error: {0}")]
    Node(String),
    #[error("internal: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, PanelError>;

impl From<sqlx::Error> for PanelError {
    fn from(e: sqlx::Error) -> Self {
        match &e {
            sqlx::Error::RowNotFound => PanelError::NotFound("record not found".to_string()),
            sqlx::Error::Database(db) if db.constraint().is_some() => {
                PanelError::Conflict(db.constraint().unwrap_or("").to_string())
            }
            _ => PanelError::Db(e.to_string()),
        }
    }
}

impl From<tonic::Status> for PanelError {
    fn from(s: tonic::Status) -> Self {
        PanelError::Node(s.message().to_string())
    }
}

impl IntoResponse for PanelError {
    fn into_response(self) -> Response {
        let status = match &self {
            PanelError::NotFound(_)     => StatusCode::NOT_FOUND,
            PanelError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            PanelError::Forbidden       => StatusCode::FORBIDDEN,
            PanelError::Conflict(_)     => StatusCode::CONFLICT,
            PanelError::Validation(_)   => StatusCode::UNPROCESSABLE_ENTITY,
            PanelError::Db(_)
            | PanelError::Node(_)
            | PanelError::Internal(_)   => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    fn status_of(err: PanelError) -> StatusCode {
        err.into_response().status()
    }

    #[test]
    fn not_found_maps_to_404() {
        assert_eq!(status_of(PanelError::NotFound("x".into())), StatusCode::NOT_FOUND);
    }

    #[test]
    fn unauthorized_maps_to_401() {
        assert_eq!(status_of(PanelError::Unauthorized("x".into())), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn forbidden_maps_to_403() {
        assert_eq!(status_of(PanelError::Forbidden), StatusCode::FORBIDDEN);
    }

    #[test]
    fn conflict_maps_to_409() {
        assert_eq!(status_of(PanelError::Conflict("x".into())), StatusCode::CONFLICT);
    }

    #[test]
    fn validation_maps_to_422() {
        assert_eq!(status_of(PanelError::Validation("x".into())), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn db_maps_to_500() {
        assert_eq!(status_of(PanelError::Db("x".into())), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
```

- [ ] **Step 6: Run tests (expect compile error, then fix)**

```bash
cargo test -p oxy-panel 2>&1 | head -30
```
Expected: compile errors until `db.rs` and `lib.rs` are created.

- [ ] **Step 7: Create db.rs**

Create `crates/panel/src/db.rs`:
```rust
use crate::error::{PanelError, Result};
use sqlx::{postgres::PgPoolOptions, PgPool};

pub async fn create_pool(database_url: &str) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .map_err(|e| PanelError::Internal(e.to_string()))
}

pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|e| PanelError::Internal(e.to_string()))
}
```

- [ ] **Step 8: Create migrations**

`crates/panel/migrations/001_users.sql`:
```sql
CREATE TABLE users (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    email         TEXT        NOT NULL UNIQUE,
    password_hash TEXT        NOT NULL,
    is_admin      BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

`crates/panel/migrations/002_nodes.sql`:
```sql
CREATE TABLE nodes (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT        NOT NULL UNIQUE,
    grpc_addr  TEXT        NOT NULL,
    token      TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

`crates/panel/migrations/003_servers.sql`:
```sql
CREATE TABLE servers (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    node_id     UUID        NOT NULL REFERENCES nodes(id) ON DELETE RESTRICT,
    name        TEXT        NOT NULL UNIQUE,
    image       TEXT        NOT NULL,
    memory_mb   INT         NOT NULL CHECK (memory_mb > 0),
    cpu_percent INT         NOT NULL CHECK (cpu_percent > 0),
    env         TEXT[]      NOT NULL DEFAULT '{}',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

- [ ] **Step 9: Scaffold lib.rs**

Replace `crates/panel/src/lib.rs`:
```rust
mod db;
pub mod error;

pub use error::{PanelError, Result};

use oxy_core::{OxyError, PanelConfig};
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub db:         PgPool,
    pub jwt_secret: String,
}

pub fn router(state: AppState) -> axum::Router {
    axum::Router::new().with_state(state)
}

pub async fn run(config: PanelConfig) -> oxy_core::Result<()> {
    let pool = db::create_pool(&config.database_url)
        .await
        .map_err(|e| OxyError::Config(e.to_string()))?;
    db::run_migrations(&pool)
        .await
        .map_err(|e| OxyError::Config(e.to_string()))?;
    let state = AppState {
        db:         pool,
        jwt_secret: config.jwt_secret,
    };
    tracing::info!(listen = %config.http_listen, "panel starting");
    let listener = tokio::net::TcpListener::bind(&config.http_listen)
        .await
        .map_err(OxyError::Io)?;
    axum::serve(listener, router(state))
        .await
        .map_err(OxyError::Io)
}
```

- [ ] **Step 10: Run tests**

```bash
cargo test -p oxy-panel 2>&1
cargo test -p oxy-core 2>&1
```
Expected: 6 PanelError tests pass; oxy-core tests still pass (after adding `jwt_secret` to test TOMLs in Step 3).

- [ ] **Step 11: Commit**

```bash
git add Cargo.toml crates/core/src/config.rs config.example.toml \
        crates/panel/Cargo.toml crates/panel/src/ crates/panel/migrations/
git commit -m "feat(oxy-panel): add PanelError, DB pool, migrations, jwt_secret config"
```

---

### Task 2: JWT auth + AuthUser/AdminUser extractors + login/refresh routes

**Files:**
- Create: `crates/panel/src/auth.rs`
- Modify: `crates/panel/src/lib.rs` (add `mod auth`, wire `/auth` routes into `router()`)

**Interfaces:**
- Consumes: `AppState { db, jwt_secret }`, `PanelError`, `Result`
- Produces:
  - `pub fn hash_password(password: &str) -> Result<String>`
  - `pub fn verify_password(password: &str, hash: &str) -> bool`
  - `pub fn encode_token(user_id: Uuid, is_admin: bool, kind: &str, secret: &str, ttl_secs: u64) -> Result<String>`
  - `pub fn decode_token(token: &str, secret: &str, expected_kind: &str) -> Result<Claims>`
  - `pub struct Claims { pub sub: String, pub adm: bool, pub exp: u64, pub kind: String }`
  - `pub struct AuthUser { pub id: Uuid, pub is_admin: bool }` — Axum `FromRequestParts<AppState>` extractor
  - `pub struct AdminUser(pub AuthUser)` — Axum `FromRequestParts<AppState>` extractor (403 if not admin)
  - `pub fn auth_router() -> Router<AppState>` — mounts `POST /login`, `POST /refresh`

- [ ] **Step 1: Write JWT + password unit tests**

Create `crates/panel/src/auth.rs` with the test module first:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret-at-least-32-chars-long!!";

    #[test]
    fn hash_and_verify_password() {
        let hash = hash_password("hunter2").unwrap();
        assert!(verify_password("hunter2", &hash));
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn encode_decode_access_token() {
        let id = Uuid::new_v4();
        let token = encode_token(id, true, "access", SECRET, 900).unwrap();
        let claims = decode_token(&token, SECRET, "access").unwrap();
        assert_eq!(claims.sub, id.to_string());
        assert!(claims.adm);
        assert_eq!(claims.kind, "access");
    }

    #[test]
    fn wrong_kind_rejected() {
        let id = Uuid::new_v4();
        let token = encode_token(id, false, "refresh", SECRET, 900).unwrap();
        assert!(decode_token(&token, SECRET, "access").is_err());
    }

    #[test]
    fn wrong_secret_rejected() {
        let id = Uuid::new_v4();
        let token = encode_token(id, false, "access", SECRET, 900).unwrap();
        assert!(decode_token(&token, "different-secret", "access").is_err());
    }

    #[test]
    fn expired_token_rejected() {
        let id = Uuid::new_v4();
        // ttl_secs = 0 produces a token that is already expired
        let token = encode_token(id, false, "access", SECRET, 0).unwrap();
        assert!(decode_token(&token, SECRET, "access").is_err());
    }
}
```

- [ ] **Step 2: Run (expect compile error)**

```bash
cargo test -p oxy-panel auth 2>&1 | head -10
```
Expected: compile error (auth module not defined yet)

- [ ] **Step 3: Implement auth.rs**

Write the full `crates/panel/src/auth.rs`:
```rust
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    async_trait,
    extract::FromRequestParts,
    http::request::Parts,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::{PanelError, Result},
    AppState,
};

// ── Password hashing ──────────────────────────────────────────────────────────

pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| PanelError::Internal(e.to_string()))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .ok()
        .map_or(false, |h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok())
}

// ── JWT ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub:  String,
    pub adm:  bool,
    pub exp:  u64,
    pub kind: String,
}

pub fn encode_token(
    user_id:  Uuid,
    is_admin: bool,
    kind:     &str,
    secret:   &str,
    ttl_secs: u64,
) -> Result<String> {
    let exp = (Utc::now().timestamp() as u64).saturating_add(ttl_secs);
    let claims = Claims {
        sub:  user_id.to_string(),
        adm:  is_admin,
        exp,
        kind: kind.to_string(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| PanelError::Internal(e.to_string()))
}

pub fn decode_token(token: &str, secret: &str, expected_kind: &str) -> Result<Claims> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|e| PanelError::Unauthorized(e.to_string()))?;
    if data.claims.kind != expected_kind {
        return Err(PanelError::Unauthorized("wrong token type".to_string()));
    }
    Ok(data.claims)
}

// ── Extractors ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id:       Uuid,
    pub is_admin: bool,
}

#[async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> std::result::Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| {
                PanelError::Unauthorized("missing Authorization header".to_string())
                    .into_response()
            })?;
        let claims = decode_token(token, &state.jwt_secret, "access")
            .map_err(IntoResponse::into_response)?;
        let id = Uuid::parse_str(&claims.sub)
            .map_err(|_| PanelError::Unauthorized("invalid sub".to_string()).into_response())?;
        Ok(AuthUser { id, is_admin: claims.adm })
    }
}

#[derive(Debug, Clone)]
pub struct AdminUser(pub AuthUser);

#[async_trait]
impl FromRequestParts<AppState> for AdminUser {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> std::result::Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state).await?;
        if !user.is_admin {
            return Err(PanelError::Forbidden.into_response());
        }
        Ok(AdminUser(user))
    }
}

// ── Login / Refresh handlers ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct LoginRequest {
    email:    String,
    password: String,
}

#[derive(Debug, Serialize)]
struct TokenResponse {
    access_token:  String,
    refresh_token: String,
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id:            Uuid,
    password_hash: String,
    is_admin:      bool,
}

pub async fn login(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(body): Json<LoginRequest>,
) -> std::result::Result<Json<TokenResponse>, PanelError> {
    let row: Option<UserRow> = sqlx::query_as::<_, UserRow>(
        "SELECT id, password_hash, is_admin FROM users WHERE email = $1",
    )
    .bind(&body.email)
    .fetch_optional(&state.db)
    .await?;

    let row = row.ok_or_else(|| PanelError::Unauthorized("invalid credentials".to_string()))?;

    let password = body.password.clone();
    let hash = row.password_hash.clone();
    let valid = tokio::task::spawn_blocking(move || verify_password(&password, &hash))
        .await
        .map_err(|e| PanelError::Internal(e.to_string()))?;

    if !valid {
        return Err(PanelError::Unauthorized("invalid credentials".to_string()));
    }

    let access_token  = encode_token(row.id, row.is_admin, "access",  &state.jwt_secret, 900)?;
    let refresh_token = encode_token(row.id, row.is_admin, "refresh", &state.jwt_secret, 604_800)?;

    Ok(Json(TokenResponse { access_token, refresh_token }))
}

#[derive(Debug, Deserialize)]
struct RefreshRequest {
    refresh_token: String,
}

#[derive(Debug, Serialize)]
struct AccessTokenResponse {
    access_token: String,
}

pub async fn refresh(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(body): Json<RefreshRequest>,
) -> std::result::Result<Json<AccessTokenResponse>, PanelError> {
    let claims = decode_token(&body.refresh_token, &state.jwt_secret, "refresh")?;
    let user_id = Uuid::parse_str(&claims.sub)
        .map_err(|_| PanelError::Unauthorized("invalid sub".to_string()))?;
    let access_token = encode_token(user_id, claims.adm, "access", &state.jwt_secret, 900)?;
    Ok(Json(AccessTokenResponse { access_token }))
}

pub fn auth_router() -> Router<AppState> {
    Router::new()
        .route("/login",   post(login))
        .route("/refresh", post(refresh))
}

#[cfg(test)]
mod tests {
    // (test module written in Step 1)
    use super::*;

    const SECRET: &str = "test-secret-at-least-32-chars-long!!";

    #[test]
    fn hash_and_verify_password() {
        let hash = hash_password("hunter2").unwrap();
        assert!(verify_password("hunter2", &hash));
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn encode_decode_access_token() {
        let id = Uuid::new_v4();
        let token = encode_token(id, true, "access", SECRET, 900).unwrap();
        let claims = decode_token(&token, SECRET, "access").unwrap();
        assert_eq!(claims.sub, id.to_string());
        assert!(claims.adm);
        assert_eq!(claims.kind, "access");
    }

    #[test]
    fn wrong_kind_rejected() {
        let id = Uuid::new_v4();
        let token = encode_token(id, false, "refresh", SECRET, 900).unwrap();
        assert!(decode_token(&token, SECRET, "access").is_err());
    }

    #[test]
    fn wrong_secret_rejected() {
        let id = Uuid::new_v4();
        let token = encode_token(id, false, "access", SECRET, 900).unwrap();
        assert!(decode_token(&token, "different-secret", "access").is_err());
    }

    #[test]
    fn expired_token_rejected() {
        let id = Uuid::new_v4();
        let token = encode_token(id, false, "access", SECRET, 0).unwrap();
        assert!(decode_token(&token, SECRET, "access").is_err());
    }
}
```

- [ ] **Step 4: Wire auth routes into lib.rs**

In `crates/panel/src/lib.rs`, add `pub mod auth;` and update `router()`:
```rust
mod db;
pub mod auth;
pub mod error;

pub use error::{PanelError, Result};

use axum::routing::post;
use oxy_core::{OxyError, PanelConfig};
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub db:         PgPool,
    pub jwt_secret: String,
}

pub fn router(state: AppState) -> axum::Router {
    axum::Router::new()
        .nest("/auth", auth::auth_router())
        .with_state(state)
}

pub async fn run(config: PanelConfig) -> oxy_core::Result<()> {
    let pool = db::create_pool(&config.database_url)
        .await
        .map_err(|e| OxyError::Config(e.to_string()))?;
    db::run_migrations(&pool)
        .await
        .map_err(|e| OxyError::Config(e.to_string()))?;
    let state = AppState {
        db:         pool,
        jwt_secret: config.jwt_secret,
    };
    tracing::info!(listen = %config.http_listen, "panel starting");
    let listener = tokio::net::TcpListener::bind(&config.http_listen)
        .await
        .map_err(OxyError::Io)?;
    axum::serve(listener, router(state))
        .await
        .map_err(OxyError::Io)
}
```

- [ ] **Step 5: Run unit tests**

```bash
cargo test -p oxy-panel 2>&1
```
Expected: 5 auth unit tests pass + 6 PanelError tests pass (11 total)

- [ ] **Step 6: Integration test for login**

Add to `crates/panel/src/auth.rs` inside `#[cfg(test)]`:
```rust
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn make_state(pool: sqlx::PgPool) -> AppState {
        AppState { db: pool, jwt_secret: SECRET.to_string() }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn login_with_valid_credentials_returns_tokens(pool: sqlx::PgPool) {
        let state = make_state(pool.clone()).await;
        let hash = hash_password("password123").unwrap();
        sqlx::query("INSERT INTO users (email, password_hash, is_admin) VALUES ($1, $2, $3)")
            .bind("admin@example.com")
            .bind(&hash)
            .bind(true)
            .execute(&pool)
            .await
            .unwrap();

        let app = crate::router(state);
        let body = serde_json::json!({ "email": "admin@example.com", "password": "password123" });
        let req = Request::builder()
            .method("POST")
            .uri("/auth/login")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), axum::http::StatusCode::OK);

        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["access_token"].is_string());
        assert!(json["refresh_token"].is_string());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn login_with_wrong_password_returns_401(pool: sqlx::PgPool) {
        let state = make_state(pool.clone()).await;
        let hash = hash_password("correct").unwrap();
        sqlx::query("INSERT INTO users (email, password_hash, is_admin) VALUES ($1, $2, $3)")
            .bind("user@example.com")
            .bind(&hash)
            .bind(false)
            .execute(&pool)
            .await
            .unwrap();

        let app = crate::router(state);
        let body = serde_json::json!({ "email": "user@example.com", "password": "wrong" });
        let req = Request::builder()
            .method("POST")
            .uri("/auth/login")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), axum::http::StatusCode::UNAUTHORIZED);
    }
```

- [ ] **Step 7: Run all tests (including DB tests)**

```bash
DATABASE_URL=postgres://oxy:oxy@localhost/oxy cargo test -p oxy-panel 2>&1
```
Expected: 13 tests pass (5 unit + 6 PanelError + 2 integration)

- [ ] **Step 8: Commit**

```bash
git add crates/panel/src/auth.rs crates/panel/src/lib.rs
git commit -m "feat(oxy-panel): JWT auth, AuthUser/AdminUser extractors, login/refresh routes"
```

---

### Task 3: User CRUD routes

**Files:**
- Create: `crates/panel/src/users.rs`
- Modify: `crates/panel/src/lib.rs` (add `mod users`, nest `/api/users` in `router()`)

**Interfaces:**
- Consumes: `AppState`, `AuthUser`, `AdminUser`, `PanelError`, `hash_password`
- Produces:
  - `#[derive(sqlx::FromRow, serde::Serialize)] pub struct User`
  - `pub fn users_router() -> Router<AppState>`
  - Routes: `GET /api/users`, `POST /api/users`, `GET /api/users/:id`, `DELETE /api/users/:id`

- [ ] **Step 1: Write tests**

Create `crates/panel/src/users.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{auth::hash_password, router, AppState};
    use axum::{body::Body, http::{Request, StatusCode}};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use uuid::Uuid;

    fn make_state(pool: sqlx::PgPool) -> AppState {
        AppState { db: pool, jwt_secret: "test-secret-at-least-32-chars-long!!".to_string() }
    }

    async fn seed_admin(pool: &sqlx::PgPool) -> (Uuid, String) {
        let id = Uuid::new_v4();
        let hash = hash_password("admin-pass").unwrap();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, is_admin) VALUES ($1, $2, $3, $4)",
        )
        .bind(id).bind("admin@test.com").bind(&hash).bind(true)
        .execute(pool).await.unwrap();
        // return JWT for admin
        let token = crate::auth::encode_token(
            id, true, "access", "test-secret-at-least-32-chars-long!!", 900,
        ).unwrap();
        (id, token)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn list_users_requires_auth(pool: sqlx::PgPool) {
        let app = router(make_state(pool));
        let req = Request::builder().method("GET").uri("/api/users")
            .body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn list_users_requires_admin(pool: sqlx::PgPool) {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, is_admin) VALUES ($1, $2, $3, $4)",
        )
        .bind(id).bind("user@test.com").bind(hash_password("pass").unwrap()).bind(false)
        .execute(&pool).await.unwrap();
        let token = crate::auth::encode_token(
            id, false, "access", "test-secret-at-least-32-chars-long!!", 900,
        ).unwrap();

        let app = router(make_state(pool));
        let req = Request::builder().method("GET").uri("/api/users")
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn admin_can_list_users(pool: sqlx::PgPool) {
        let (_id, token) = seed_admin(&pool).await;
        let app = router(make_state(pool));
        let req = Request::builder().method("GET").uri("/api/users")
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json.as_array().unwrap().len() >= 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn admin_can_create_user(pool: sqlx::PgPool) {
        let (_id, token) = seed_admin(&pool).await;
        let app = router(make_state(pool));
        let body = serde_json::json!({
            "email": "new@test.com",
            "password": "newpassword",
            "is_admin": false
        });
        let req = Request::builder().method("POST").uri("/api/users")
            .header("authorization", format!("Bearer {}", token))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
    }
}
```

- [ ] **Step 2: Run (expect compile error)**

```bash
cargo test -p oxy-panel users 2>&1 | head -10
```

- [ ] **Step 3: Implement users.rs**

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::{hash_password, AdminUser, AuthUser},
    error::{PanelError, Result},
    AppState,
};

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct User {
    pub id:         Uuid,
    pub email:      String,
    #[serde(skip)]
    pub password_hash: String,
    pub is_admin:   bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct CreateUserRequest {
    email:    String,
    password: String,
    is_admin: bool,
}

async fn list_users(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<Vec<User>>> {
    let users = sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, is_admin, created_at FROM users ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(users))
}

async fn get_user(
    State(state): State<AppState>,
    caller: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<User>> {
    if !caller.is_admin && caller.id != id {
        return Err(PanelError::Forbidden);
    }
    let user = sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, is_admin, created_at FROM users WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(user))
}

async fn create_user(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<User>)> {
    if body.email.is_empty() || body.password.len() < 8 {
        return Err(PanelError::Validation(
            "email required; password must be at least 8 characters".to_string(),
        ));
    }
    let password = body.password.clone();
    let hash = tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(|e| PanelError::Internal(e.to_string()))??;

    let user = sqlx::query_as::<_, User>(
        "INSERT INTO users (email, password_hash, is_admin)
         VALUES ($1, $2, $3)
         RETURNING id, email, password_hash, is_admin, created_at",
    )
    .bind(&body.email)
    .bind(&hash)
    .bind(body.is_admin)
    .fetch_one(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(user)))
}

async fn delete_user(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let rows = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if rows == 0 {
        return Err(PanelError::NotFound(id.to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub fn users_router() -> Router<AppState> {
    Router::new()
        .route("/",    get(list_users).post(create_user))
        .route("/:id", get(get_user).delete(delete_user))
}

#[cfg(test)]
mod tests {
    // (written in Step 1)
}
```

- [ ] **Step 4: Wire into lib.rs**

In `crates/panel/src/lib.rs`, add `mod users;` and update `router()`:
```rust
mod db;
pub mod auth;
pub mod error;
mod users;

// ... AppState unchanged ...

pub fn router(state: AppState) -> axum::Router {
    axum::Router::new()
        .nest("/auth",      auth::auth_router())
        .nest("/api/users", users::users_router())
        .with_state(state)
}
```

- [ ] **Step 5: Run tests**

```bash
DATABASE_URL=postgres://oxy:oxy@localhost/oxy cargo test -p oxy-panel 2>&1
```
Expected: all prior tests pass + 4 new user tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/panel/src/users.rs crates/panel/src/lib.rs
git commit -m "feat(oxy-panel): user CRUD routes (list, get, create, delete)"
```

---

### Task 4: Node CRUD routes

**Files:**
- Create: `crates/panel/src/nodes.rs`
- Modify: `crates/panel/src/lib.rs` (add `mod nodes`, nest `/api/nodes` in `router()`)

**Interfaces:**
- Consumes: `AppState`, `AdminUser`, `PanelError`
- Produces:
  - `#[derive(sqlx::FromRow, serde::Serialize, Clone)] pub struct Node`
  - `pub fn nodes_router() -> Router<AppState>`
  - Routes: `GET /api/nodes`, `POST /api/nodes`, `DELETE /api/nodes/:id`

- [ ] **Step 1: Write tests**

Create `crates/panel/src/nodes.rs` with tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{auth::{encode_token, hash_password}, router, AppState};
    use axum::{body::Body, http::{Request, StatusCode}};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use uuid::Uuid;

    const SECRET: &str = "test-secret-at-least-32-chars-long!!";

    fn make_state(pool: sqlx::PgPool) -> AppState {
        AppState { db: pool, jwt_secret: SECRET.to_string() }
    }

    async fn seed_admin(pool: &sqlx::PgPool) -> String {
        let id = Uuid::new_v4();
        let hash = hash_password("pass").unwrap();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, is_admin) VALUES ($1, $2, $3, $4)",
        )
        .bind(id).bind("a@t.com").bind(&hash).bind(true)
        .execute(pool).await.unwrap();
        encode_token(id, true, "access", SECRET, 900).unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_node_and_list(pool: sqlx::PgPool) {
        let token = seed_admin(&pool).await;
        let app = router(make_state(pool));

        let body = serde_json::json!({
            "name": "node-eu-1",
            "grpc_addr": "http://10.0.0.1:8080",
            "token": "secret-node-token"
        });
        let create_req = Request::builder()
            .method("POST").uri("/api/nodes")
            .header("authorization", format!("Bearer {}", token))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap();
        let res = app.clone().oneshot(create_req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);

        let list_req = Request::builder()
            .method("GET").uri("/api/nodes")
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty()).unwrap();
        let res = app.oneshot(list_req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json.as_array().unwrap().len(), 1);
        assert_eq!(json[0]["name"], "node-eu-1");
        assert!(json[0].get("token").is_none(), "token must not be serialized");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn delete_node_returns_204(pool: sqlx::PgPool) {
        let token = seed_admin(&pool).await;
        // insert a node directly
        let node_id: Uuid = sqlx::query_scalar(
            "INSERT INTO nodes (name, grpc_addr, token) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind("n1").bind("http://localhost:8080").bind("tok")
        .fetch_one(&pool).await.unwrap();

        let app = router(make_state(pool));
        let req = Request::builder()
            .method("DELETE").uri(format!("/api/nodes/{}", node_id))
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
    }
}
```

- [ ] **Step 2: Run (expect compile error)**

```bash
cargo test -p oxy-panel nodes 2>&1 | head -10
```

- [ ] **Step 3: Implement nodes.rs**

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::AdminUser,
    error::{PanelError, Result},
    AppState,
};

#[derive(Debug, sqlx::FromRow, Serialize, Clone)]
pub struct Node {
    pub id:        Uuid,
    pub name:      String,
    pub grpc_addr: String,
    #[serde(skip)]
    pub token:     String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct CreateNodeRequest {
    name:      String,
    grpc_addr: String,
    token:     String,
}

async fn list_nodes(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<Vec<Node>>> {
    let nodes = sqlx::query_as::<_, Node>(
        "SELECT id, name, grpc_addr, token, created_at FROM nodes ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(nodes))
}

async fn create_node(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<CreateNodeRequest>,
) -> Result<(StatusCode, Json<Node>)> {
    if body.name.is_empty() || body.grpc_addr.is_empty() || body.token.is_empty() {
        return Err(PanelError::Validation("name, grpc_addr, and token are required".to_string()));
    }
    let node = sqlx::query_as::<_, Node>(
        "INSERT INTO nodes (name, grpc_addr, token)
         VALUES ($1, $2, $3)
         RETURNING id, name, grpc_addr, token, created_at",
    )
    .bind(&body.name)
    .bind(&body.grpc_addr)
    .bind(&body.token)
    .fetch_one(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(node)))
}

async fn delete_node(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let rows = sqlx::query("DELETE FROM nodes WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if rows == 0 {
        return Err(PanelError::NotFound(id.to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub fn nodes_router() -> Router<AppState> {
    Router::new()
        .route("/",    get(list_nodes).post(create_node))
        .route("/:id", delete(delete_node))
}

#[cfg(test)]
mod tests {
    // (written in Step 1)
}
```

- [ ] **Step 4: Wire into lib.rs**

Add `mod nodes;` and update `router()`:
```rust
pub fn router(state: AppState) -> axum::Router {
    axum::Router::new()
        .nest("/auth",      auth::auth_router())
        .nest("/api/users", users::users_router())
        .nest("/api/nodes", nodes::nodes_router())
        .with_state(state)
}
```

Also add `mod nodes;` with the other mods at the top.

- [ ] **Step 5: Run tests**

```bash
DATABASE_URL=postgres://oxy:oxy@localhost/oxy cargo test -p oxy-panel 2>&1
```
Expected: all prior + 2 new node tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/panel/src/nodes.rs crates/panel/src/lib.rs
git commit -m "feat(oxy-panel): node CRUD routes (list, create, delete)"
```

---

### Task 5: ProvisionServer RPC — proto + node impl

**Files:**
- Modify: `crates/core/proto/oxydactylus.proto` (add `ProvisionServer` RPC + message)
- Modify: `crates/node/src/server.rs` (implement `provision_server` + add test)

**Interfaces:**
- Consumes: `ContainerSpec { image, name, env, memory_mb, cpu_percent }` (already has `env: Vec<String>`)
- Produces:
  - `ServerProvisionRequest { server_id, image, memory_mb, cpu_percent, env }` proto message
  - `NodeService::provision_server` RPC
  - `NodeServiceClient::provision_server` callable from panel (Task 6)

- [ ] **Step 1: Write node server test first**

In `crates/node/src/server.rs`, add to the `tests` module:
```rust
    #[tokio::test]
    async fn provision_server_creates_container() {
        let mut mock = MockDockerBackend::new();
        mock.expect_create_container()
            .once()
            .returning(|spec| {
                async move {
                    assert_eq!(spec.name, "srv-new");
                    assert_eq!(spec.image, "itzg/minecraft-server");
                    assert_eq!(spec.memory_mb, 1024);
                    assert_eq!(spec.cpu_percent, 100);
                    assert_eq!(spec.env, vec!["EULA=TRUE"]);
                    Ok("container-id-xyz".to_string())
                }.boxed()
            });

        let reply = svc(mock)
            .provision_server(Request::new(
                oxy_core::proto::node::ServerProvisionRequest {
                    server_id:   "srv-new".into(),
                    image:       "itzg/minecraft-server".into(),
                    memory_mb:   1024,
                    cpu_percent: 100,
                    env:         vec!["EULA=TRUE".into()],
                },
            ))
            .await
            .unwrap()
            .into_inner();

        assert!(reply.success);
    }
```

- [ ] **Step 2: Run (expect compile error — no ProvisionServer yet)**

```bash
cargo test -p oxy-node provision 2>&1 | head -20
```

- [ ] **Step 3: Add ProvisionServer to proto**

In `crates/core/proto/oxydactylus.proto`, add the new RPC to the `NodeService` service:
```proto
rpc ProvisionServer (ServerProvisionRequest) returns (ServerReply);
```

And add the new message at the bottom of the file:
```proto
message ServerProvisionRequest {
    string          server_id   = 1;
    string          image       = 2;
    uint32          memory_mb   = 3;
    uint32          cpu_percent = 4;
    repeated string env         = 5;
}
```

The complete updated proto:
```proto
syntax = "proto3";

package oxydactylus.node;

service NodeService {
    rpc StartServer      (ServerStartRequest)     returns (ServerReply);
    rpc StopServer       (ServerStopRequest)      returns (ServerReply);
    rpc DeleteServer     (ServerDeleteRequest)    returns (ServerReply);
    rpc GetStats         (ServerStatsRequest)     returns (ServerStats);
    rpc StreamLogs       (ServerLogsRequest)      returns (stream LogLine);
    rpc SendCommand      (ServerCommandRequest)   returns (ServerReply);
    rpc ProvisionServer  (ServerProvisionRequest) returns (ServerReply);
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

message ServerProvisionRequest {
    string          server_id   = 1;
    string          image       = 2;
    uint32          memory_mb   = 3;
    uint32          cpu_percent = 4;
    repeated string env         = 5;
}
```

- [ ] **Step 4: Implement provision_server in NodeServiceImpl**

In `crates/node/src/server.rs`, add the import for `ServerProvisionRequest` in the `use` block at the top:
```rust
use oxy_core::proto::node::{
    node_service_server::NodeService,
    LogLine, ServerCommandRequest, ServerDeleteRequest, ServerLogsRequest,
    ServerProvisionRequest, ServerReply, ServerStartRequest, ServerStats,
    ServerStatsRequest, ServerStopRequest,
};
```

Then add the `provision_server` method to the `impl<B: DockerBackend> NodeService for NodeServiceImpl<B>` block (after `send_command`):
```rust
    async fn provision_server(
        &self,
        req: Request<ServerProvisionRequest>,
    ) -> Result<Response<ServerReply>, Status> {
        let r = req.into_inner();
        use crate::docker::ContainerSpec;
        self.docker
            .create_container(ContainerSpec {
                name:        r.server_id.clone(),
                image:       r.image,
                memory_mb:   r.memory_mb as i64,
                cpu_percent: r.cpu_percent as i64,
                env:         r.env,
            })
            .await
            .map_err(Status::from)?;
        Ok(Self::ok(format!("provisioned {}", r.server_id)))
    }
```

- [ ] **Step 5: Run node tests**

```bash
cargo test -p oxy-node 2>&1
```
Expected: all 21 existing tests + 1 new `provision_server_creates_container` = 22 tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/core/proto/oxydactylus.proto crates/node/src/server.rs
git commit -m "feat(oxy-node): add ProvisionServer RPC to proto and NodeServiceImpl"
```

---

### Task 6: NodeClient gRPC wrapper (panel side)

**Files:**
- Create: `crates/panel/src/node_client.rs`
- Modify: `crates/panel/src/lib.rs` (add `pub mod node_client`)

**Interfaces:**
- Consumes: `oxy_core::proto::node::*` (tonic generated types), `PanelError`
- Produces:
  - `pub struct NodeClient` — wraps `NodeServiceClient<InterceptedService<Channel, BearerInterceptor>>`
  - `pub async fn NodeClient::connect(grpc_addr: &str, token: &str) -> Result<NodeClient>`
  - Methods (all `&mut self`): `provision`, `start`, `stop`, `delete`, `send_command`, `get_stats`
  - All methods return `Result<T, PanelError>` where T is `()` or `ServerStats`

Note: In tonic 0.12, generated client methods take `&mut self`. The `NodeClient` is short-lived (created per request), so mutability is fine.

- [ ] **Step 1: Write integration test with mock gRPC server**

Create `crates/panel/src/node_client.rs` with the test module:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::future::FutureExt;
    use oxy_core::proto::node::{
        node_service_server::{NodeService, NodeServiceServer},
        ServerProvisionRequest, ServerReply, ServerStartRequest,
        ServerStopRequest, ServerDeleteRequest, ServerCommandRequest,
        ServerStatsRequest, ServerStats, LogLine, ServerLogsRequest,
    };
    use tonic::{async_trait, Request, Response, Status};
    use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};

    struct EchoNode;

    #[async_trait]
    impl NodeService for EchoNode {
        type StreamLogsStream = ReceiverStream<Result<LogLine, Status>>;

        async fn provision_server(&self, _: Request<ServerProvisionRequest>)
            -> Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }

        async fn start_server(&self, _: Request<ServerStartRequest>)
            -> Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "started".into() })) }

        async fn stop_server(&self, _: Request<ServerStopRequest>)
            -> Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "stopped".into() })) }

        async fn delete_server(&self, _: Request<ServerDeleteRequest>)
            -> Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "deleted".into() })) }

        async fn send_command(&self, _: Request<ServerCommandRequest>)
            -> Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "sent".into() })) }

        async fn get_stats(&self, req: Request<ServerStatsRequest>)
            -> Result<Response<ServerStats>, Status>
        {
            let id = req.into_inner().server_id;
            Ok(Response::new(ServerStats {
                server_id: id, memory_bytes: 512, cpu_percent: 5.0,
                rx_bytes: 100, tx_bytes: 200,
            }))
        }

        async fn stream_logs(&self, _: Request<ServerLogsRequest>)
            -> Result<Response<Self::StreamLogsStream>, Status>
        {
            let (_, rx) = tokio::sync::mpsc::channel(1);
            Ok(Response::new(ReceiverStream::new(rx)))
        }
    }

    async fn start_test_server(token: &str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let token_clone = token.to_string();

        tokio::spawn(async move {
            use oxy_node::interceptor::AuthInterceptor;
            let interceptor = AuthInterceptor::new(&token_clone);
            tonic::transport::Server::builder()
                .add_service(NodeServiceServer::with_interceptor(EchoNode, interceptor))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        });

        format!("http://127.0.0.1:{}", addr.port())
    }

    #[tokio::test]
    async fn client_can_provision_and_start() {
        let token = "test-node-token";
        let addr = start_test_server(token).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = NodeClient::connect(&addr, token).await.unwrap();
        client.provision("srv-1", "ubuntu:latest", 512, 50, vec!["X=1".into()]).await.unwrap();
        client.start("srv-1").await.unwrap();
    }

    #[tokio::test]
    async fn client_gets_stats() {
        let token = "test-token-2";
        let addr = start_test_server(token).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = NodeClient::connect(&addr, token).await.unwrap();
        let stats = client.get_stats("srv-x").await.unwrap();
        assert_eq!(stats.server_id, "srv-x");
        assert_eq!(stats.memory_bytes, 512);
    }

    #[tokio::test]
    async fn wrong_token_returns_node_error() {
        let addr = start_test_server("correct-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = NodeClient::connect(&addr, "wrong-token").await.unwrap();
        let err = client.start("srv-1").await.unwrap_err();
        assert!(matches!(err, PanelError::Node(_)));
    }
}
```

- [ ] **Step 2: Run (expect compile error)**

```bash
cargo test -p oxy-panel node_client 2>&1 | head -20
```

- [ ] **Step 3: Add oxy-node as dev-dependency in panel**

The test uses `oxy_node::interceptor::AuthInterceptor`. Add to `crates/panel/Cargo.toml`:
```toml
[dev-dependencies]
sqlx           = { workspace = true, features = ["test"] }
tower          = { workspace = true }
http-body-util = "0.1"
oxy-node       = { path = "../node" }
tokio-stream   = { workspace = true, features = ["net"] }
```

- [ ] **Step 4: Implement node_client.rs**

Write the full `crates/panel/src/node_client.rs`:
```rust
use oxy_core::proto::node::{
    node_service_client::NodeServiceClient,
    ServerCommandRequest, ServerDeleteRequest, ServerProvisionRequest,
    ServerStartRequest, ServerStats, ServerStatsRequest, ServerStopRequest,
};
use tonic::{
    metadata::MetadataValue,
    service::interceptor::InterceptedService,
    transport::Channel,
    Request,
};

use crate::error::{PanelError, Result};

struct BearerInterceptor {
    token: String,
}

impl tonic::service::Interceptor for BearerInterceptor {
    fn call(
        &mut self,
        mut req: Request<()>,
    ) -> std::result::Result<Request<()>, tonic::Status> {
        let val = MetadataValue::try_from(format!("Bearer {}", self.token))
            .map_err(|_| tonic::Status::internal("invalid token format"))?;
        req.metadata_mut().insert("authorization", val);
        Ok(req)
    }
}

pub struct NodeClient {
    inner: NodeServiceClient<InterceptedService<Channel, BearerInterceptor>>,
}

impl NodeClient {
    pub async fn connect(grpc_addr: &str, token: &str) -> Result<Self> {
        let channel = Channel::from_shared(grpc_addr.to_string())
            .map_err(|e| PanelError::Node(e.to_string()))?
            .connect()
            .await
            .map_err(|e| PanelError::Node(e.to_string()))?;
        let interceptor = BearerInterceptor { token: token.to_string() };
        Ok(Self { inner: NodeServiceClient::with_interceptor(channel, interceptor) })
    }

    pub async fn provision(
        &mut self,
        server_id:   &str,
        image:       &str,
        memory_mb:   u32,
        cpu_percent: u32,
        env:         Vec<String>,
    ) -> Result<()> {
        self.inner
            .provision_server(ServerProvisionRequest {
                server_id:   server_id.to_string(),
                image:       image.to_string(),
                memory_mb,
                cpu_percent,
                env,
            })
            .await
            .map(|_| ())
            .map_err(PanelError::from)
    }

    pub async fn start(&mut self, server_id: &str) -> Result<()> {
        self.inner
            .start_server(ServerStartRequest { server_id: server_id.to_string() })
            .await
            .map(|_| ())
            .map_err(PanelError::from)
    }

    pub async fn stop(&mut self, server_id: &str, timeout: u32) -> Result<()> {
        self.inner
            .stop_server(ServerStopRequest {
                server_id: server_id.to_string(),
                timeout,
            })
            .await
            .map(|_| ())
            .map_err(PanelError::from)
    }

    pub async fn delete(&mut self, server_id: &str) -> Result<()> {
        self.inner
            .delete_server(ServerDeleteRequest { server_id: server_id.to_string() })
            .await
            .map(|_| ())
            .map_err(PanelError::from)
    }

    pub async fn send_command(&mut self, server_id: &str, content: &str) -> Result<()> {
        self.inner
            .send_command(ServerCommandRequest {
                server_id: server_id.to_string(),
                content:   content.to_string(),
            })
            .await
            .map(|_| ())
            .map_err(PanelError::from)
    }

    pub async fn get_stats(&mut self, server_id: &str) -> Result<ServerStats> {
        self.inner
            .get_stats(ServerStatsRequest { server_id: server_id.to_string() })
            .await
            .map(|r| r.into_inner())
            .map_err(PanelError::from)
    }
}

#[cfg(test)]
mod tests {
    // (written in Step 1)
}
```

- [ ] **Step 5: Add mod to lib.rs**

In `crates/panel/src/lib.rs`, add:
```rust
pub mod node_client;
```

- [ ] **Step 6: Run tests**

```bash
DATABASE_URL=postgres://oxy:oxy@localhost/oxy cargo test -p oxy-panel 2>&1
```
Expected: all prior tests + 3 new NodeClient integration tests pass

- [ ] **Step 7: Commit**

```bash
git add crates/panel/src/node_client.rs crates/panel/src/lib.rs crates/panel/Cargo.toml
git commit -m "feat(oxy-panel): NodeClient gRPC wrapper with bearer token interceptor"
```

---

### Task 7: Server CRUD + lifecycle proxy routes

**Files:**
- Create: `crates/panel/src/servers.rs`
- Modify: `crates/panel/src/lib.rs` (add `mod servers`, nest `/api/servers` in `router()`)

**Interfaces:**
- Consumes: `AppState`, `AdminUser`, `AuthUser`, `PanelError`, `NodeClient`, `nodes::Node` (query from DB)
- Produces:
  - `#[derive(sqlx::FromRow, serde::Serialize)] pub struct Server`
  - `pub fn servers_router() -> Router<AppState>`
  - Routes:
    - `GET    /api/servers`          → list servers (AuthUser)
    - `POST   /api/servers`          → create + provision on node (AdminUser)
    - `GET    /api/servers/:id`      → get server (AuthUser)
    - `DELETE /api/servers/:id`      → stop + delete container + delete DB record (AdminUser)
    - `POST   /api/servers/:id/start`   → start container on node (AuthUser)
    - `POST   /api/servers/:id/stop`    → stop container on node (AuthUser)
    - `POST   /api/servers/:id/command` → send command to container (AuthUser)
    - `GET    /api/servers/:id/stats`   → get stats from node (AuthUser)

**Design notes:**
- `POST /api/servers` calls `NodeClient::provision` on the server's node then returns 201
- `DELETE /api/servers/:id` calls `NodeClient::stop` (ignores 404 from node), then `NodeClient::delete`, then deletes DB record
- The container name used in all node calls is `server.id.to_string()`
- Node lookup: given `server.node_id`, query `nodes` table for `grpc_addr` and `token`

- [ ] **Step 1: Write tests**

Create `crates/panel/src/servers.rs` with tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        auth::{encode_token, hash_password},
        router, AppState,
    };
    use axum::{body::Body, http::{Request, StatusCode}};
    use futures_util::future::FutureExt;
    use http_body_util::BodyExt;
    use oxy_core::proto::node::{
        node_service_server::{NodeService, NodeServiceServer},
        LogLine, ServerCommandRequest, ServerDeleteRequest, ServerLogsRequest,
        ServerProvisionRequest, ServerReply, ServerStartRequest, ServerStatsRequest,
        ServerStats, ServerStopRequest,
    };
    use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
    use tonic::{async_trait, Request as GrpcRequest, Response, Status};
    use tower::ServiceExt;
    use uuid::Uuid;

    const SECRET: &str = "test-secret-at-least-32-chars-long!!";

    fn make_state(pool: sqlx::PgPool) -> AppState {
        AppState { db: pool, jwt_secret: SECRET.to_string() }
    }

    async fn seed_admin(pool: &sqlx::PgPool) -> (Uuid, String) {
        let id = Uuid::new_v4();
        let hash = hash_password("pass").unwrap();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, is_admin) VALUES ($1, $2, $3, $4)",
        )
        .bind(id).bind("a@t.com").bind(&hash).bind(true)
        .execute(pool).await.unwrap();
        let token = encode_token(id, true, "access", SECRET, 900).unwrap();
        (id, token)
    }

    struct AcceptAllNode;

    #[async_trait]
    impl NodeService for AcceptAllNode {
        type StreamLogsStream = ReceiverStream<Result<LogLine, Status>>;
        async fn provision_server(&self, _: GrpcRequest<ServerProvisionRequest>)
            -> Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
        async fn start_server(&self, _: GrpcRequest<ServerStartRequest>)
            -> Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
        async fn stop_server(&self, _: GrpcRequest<ServerStopRequest>)
            -> Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
        async fn delete_server(&self, _: GrpcRequest<ServerDeleteRequest>)
            -> Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
        async fn send_command(&self, _: GrpcRequest<ServerCommandRequest>)
            -> Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
        async fn get_stats(&self, req: GrpcRequest<ServerStatsRequest>)
            -> Result<Response<ServerStats>, Status>
        {
            Ok(Response::new(ServerStats {
                server_id: req.into_inner().server_id,
                memory_bytes: 1024, cpu_percent: 10.0,
                rx_bytes: 50, tx_bytes: 100,
            }))
        }
        async fn stream_logs(&self, _: GrpcRequest<ServerLogsRequest>)
            -> Result<Response<Self::StreamLogsStream>, Status>
        { let (_, rx) = tokio::sync::mpsc::channel(1); Ok(Response::new(ReceiverStream::new(rx))) }
    }

    async fn start_mock_node(token: &str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let t = token.to_string();
        tokio::spawn(async move {
            use oxy_node::interceptor::AuthInterceptor;
            tonic::transport::Server::builder()
                .add_service(NodeServiceServer::with_interceptor(AcceptAllNode, AuthInterceptor::new(&t)))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        });
        format!("http://127.0.0.1:{}", port)
    }

    async fn seed_node(pool: &sqlx::PgPool, grpc_addr: &str) -> Uuid {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO nodes (name, grpc_addr, token) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind("test-node").bind(grpc_addr).bind("node-token")
        .fetch_one(pool).await.unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_server_provisions_on_node(pool: sqlx::PgPool) {
        let node_addr = start_mock_node("node-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (_, token) = seed_admin(&pool).await;
        let node_id = seed_node(&pool, &node_addr).await;

        let app = router(make_state(pool));
        let body = serde_json::json!({
            "node_id":     node_id,
            "name":        "mc-server-1",
            "image":       "itzg/minecraft-server",
            "memory_mb":   1024,
            "cpu_percent": 100,
            "env":         ["EULA=TRUE"]
        });
        let req = Request::builder()
            .method("POST").uri("/api/servers")
            .header("authorization", format!("Bearer {}", token))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn list_servers_returns_empty(pool: sqlx::PgPool) {
        let (_, token) = seed_admin(&pool).await;
        let app = router(make_state(pool));
        let req = Request::builder()
            .method("GET").uri("/api/servers")
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json.as_array().unwrap().len(), 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_stats_proxies_to_node(pool: sqlx::PgPool) {
        let node_addr = start_mock_node("node-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (_, token) = seed_admin(&pool).await;
        let node_id = seed_node(&pool, &node_addr).await;
        // Insert server directly
        let server_id: Uuid = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO servers (node_id, name, image, memory_mb, cpu_percent)
             VALUES ($1, $2, $3, $4, $5) RETURNING id",
        )
        .bind(node_id).bind("srv-x").bind("ubuntu").bind(512).bind(50)
        .fetch_one(&pool).await.unwrap();

        let app = router(make_state(pool));
        let req = Request::builder()
            .method("GET").uri(format!("/api/servers/{}/stats", server_id))
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["memory_bytes"], 1024);
    }
}
```

- [ ] **Step 2: Run (expect compile error)**

```bash
cargo test -p oxy-panel servers 2>&1 | head -20
```

- [ ] **Step 3: Implement servers.rs**

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::{AdminUser, AuthUser},
    error::{PanelError, Result},
    node_client::NodeClient,
    AppState,
};

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

#[derive(Debug, sqlx::FromRow)]
struct NodeRow {
    grpc_addr: String,
    token:     String,
}

async fn get_node_client(state: &AppState, node_id: Uuid) -> Result<NodeClient> {
    let row = sqlx::query_as::<_, NodeRow>(
        "SELECT grpc_addr, token FROM nodes WHERE id = $1",
    )
    .bind(node_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| PanelError::NotFound(format!("node {}", node_id)))?;
    NodeClient::connect(&row.grpc_addr, &row.token).await
}

async fn list_servers(
    State(state): State<AppState>,
    _user: AuthUser,
) -> Result<Json<Vec<Server>>> {
    let servers = sqlx::query_as::<_, Server>(
        "SELECT id, node_id, name, image, memory_mb, cpu_percent, env, created_at
         FROM servers ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(servers))
}

#[derive(Debug, Deserialize)]
struct CreateServerRequest {
    node_id:     Uuid,
    name:        String,
    image:       String,
    memory_mb:   i32,
    cpu_percent: i32,
    #[serde(default)]
    env:         Vec<String>,
}

async fn create_server(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<CreateServerRequest>,
) -> Result<(StatusCode, Json<Server>)> {
    if body.memory_mb <= 0 || body.cpu_percent <= 0 {
        return Err(PanelError::Validation(
            "memory_mb and cpu_percent must be positive".to_string(),
        ));
    }
    if body.name.is_empty() || body.image.is_empty() {
        return Err(PanelError::Validation("name and image are required".to_string()));
    }

    let server = sqlx::query_as::<_, Server>(
        "INSERT INTO servers (node_id, name, image, memory_mb, cpu_percent, env)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id, node_id, name, image, memory_mb, cpu_percent, env, created_at",
    )
    .bind(body.node_id)
    .bind(&body.name)
    .bind(&body.image)
    .bind(body.memory_mb)
    .bind(body.cpu_percent)
    .bind(&body.env)
    .fetch_one(&state.db)
    .await?;

    let mut client = get_node_client(&state, server.node_id).await?;
    client.provision(
        &server.id.to_string(),
        &server.image,
        server.memory_mb as u32,
        server.cpu_percent as u32,
        server.env.clone(),
    )
    .await?;

    Ok((StatusCode::CREATED, Json(server)))
}

async fn get_server(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Server>> {
    let server = sqlx::query_as::<_, Server>(
        "SELECT id, node_id, name, image, memory_mb, cpu_percent, env, created_at
         FROM servers WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(server))
}

async fn delete_server(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let server = sqlx::query_as::<_, Server>(
        "SELECT id, node_id, name, image, memory_mb, cpu_percent, env, created_at
         FROM servers WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    if let Ok(mut client) = get_node_client(&state, server.node_id).await {
        let _ = client.stop(&server.id.to_string(), 10).await;
        let _ = client.delete(&server.id.to_string()).await;
    }

    sqlx::query("DELETE FROM servers WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn start_server(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let server = sqlx::query_as::<_, Server>(
        "SELECT id, node_id, name, image, memory_mb, cpu_percent, env, created_at
         FROM servers WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    let mut client = get_node_client(&state, server.node_id).await?;
    client.start(&server.id.to_string()).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn stop_server(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let server = sqlx::query_as::<_, Server>(
        "SELECT id, node_id, name, image, memory_mb, cpu_percent, env, created_at
         FROM servers WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    let mut client = get_node_client(&state, server.node_id).await?;
    client.stop(&server.id.to_string(), 10).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn server_command(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<StatusCode> {
    let content = body["content"]
        .as_str()
        .ok_or_else(|| PanelError::Validation("content field required".to_string()))?
        .to_string();
    let server = sqlx::query_as::<_, Server>(
        "SELECT id, node_id, name, image, memory_mb, cpu_percent, env, created_at
         FROM servers WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    let mut client = get_node_client(&state, server.node_id).await?;
    client.send_command(&server.id.to_string(), &content).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn server_stats(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    let server = sqlx::query_as::<_, Server>(
        "SELECT id, node_id, name, image, memory_mb, cpu_percent, env, created_at
         FROM servers WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    let mut client = get_node_client(&state, server.node_id).await?;
    let stats = client.get_stats(&server.id.to_string()).await?;
    Ok(Json(serde_json::json!({
        "memory_bytes": stats.memory_bytes,
        "cpu_percent":  stats.cpu_percent,
        "rx_bytes":     stats.rx_bytes,
        "tx_bytes":     stats.tx_bytes,
    })))
}

pub fn servers_router() -> Router<AppState> {
    Router::new()
        .route("/",              get(list_servers).post(create_server))
        .route("/:id",           get(get_server).delete(delete_server))
        .route("/:id/start",     post(start_server))
        .route("/:id/stop",      post(stop_server))
        .route("/:id/command",   post(server_command))
        .route("/:id/stats",     get(server_stats))
}

#[cfg(test)]
mod tests {
    // (written in Step 1)
}
```

- [ ] **Step 4: Wire into lib.rs**

Add `mod servers;` and update `router()`:
```rust
mod db;
pub mod auth;
pub mod error;
pub mod node_client;
mod nodes;
mod servers;
mod users;

// ...

pub fn router(state: AppState) -> axum::Router {
    axum::Router::new()
        .nest("/auth",        auth::auth_router())
        .nest("/api/users",   users::users_router())
        .nest("/api/nodes",   nodes::nodes_router())
        .nest("/api/servers", servers::servers_router())
        .with_state(state)
}
```

- [ ] **Step 5: Run tests**

```bash
DATABASE_URL=postgres://oxy:oxy@localhost/oxy cargo test -p oxy-panel 2>&1
```
Expected: all prior tests + 3 new server tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/panel/src/servers.rs crates/panel/src/lib.rs
git commit -m "feat(oxy-panel): server CRUD routes and lifecycle proxy to node gRPC"
```

---

### Task 8: Wire run() + full integration test

**Files:**
- Modify: `crates/panel/src/lib.rs` (final `run()` — already correct shape, verify no stubs remain)
- Create: `crates/panel/tests/integration.rs` (or add to `lib.rs` test module)

**Interfaces:**
- Consumes: all prior modules
- Produces: full panel HTTP server round-trip test (real DB + mock gRPC node)

- [ ] **Step 1: Write end-to-end integration test**

Create `crates/panel/tests/integration.rs`:
```rust
use axum::{body::Body, http::{Request, StatusCode}};
use http_body_util::BodyExt;
use oxy_core::proto::node::{
    node_service_server::{NodeService, NodeServiceServer},
    LogLine, ServerCommandRequest, ServerDeleteRequest, ServerLogsRequest,
    ServerProvisionRequest, ServerReply, ServerStartRequest, ServerStats,
    ServerStatsRequest, ServerStopRequest,
};
use oxy_panel::{auth::{encode_token, hash_password}, router, AppState};
use sqlx::PgPool;
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
use tonic::{async_trait, Request as GrpcRequest, Response, Status};
use tower::ServiceExt;
use uuid::Uuid;

const SECRET: &str = "integration-test-secret-32-chars!!";

struct OkNode;

#[async_trait]
impl NodeService for OkNode {
    type StreamLogsStream = ReceiverStream<Result<LogLine, Status>>;
    async fn provision_server(&self, _: GrpcRequest<ServerProvisionRequest>)
        -> Result<Response<ServerReply>, Status>
    { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
    async fn start_server(&self, _: GrpcRequest<ServerStartRequest>)
        -> Result<Response<ServerReply>, Status>
    { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
    async fn stop_server(&self, _: GrpcRequest<ServerStopRequest>)
        -> Result<Response<ServerReply>, Status>
    { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
    async fn delete_server(&self, _: GrpcRequest<ServerDeleteRequest>)
        -> Result<Response<ServerReply>, Status>
    { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
    async fn send_command(&self, _: GrpcRequest<ServerCommandRequest>)
        -> Result<Response<ServerReply>, Status>
    { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }
    async fn get_stats(&self, req: GrpcRequest<ServerStatsRequest>)
        -> Result<Response<ServerStats>, Status>
    {
        Ok(Response::new(ServerStats {
            server_id: req.into_inner().server_id,
            memory_bytes: 256, cpu_percent: 3.0, rx_bytes: 10, tx_bytes: 20,
        }))
    }
    async fn stream_logs(&self, _: GrpcRequest<ServerLogsRequest>)
        -> Result<Response<Self::StreamLogsStream>, Status>
    { let (_, rx) = tokio::sync::mpsc::channel(1); Ok(Response::new(ReceiverStream::new(rx))) }
}

async fn start_node(token: &str) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let t = token.to_string();
    tokio::spawn(async move {
        use oxy_node::interceptor::AuthInterceptor;
        tonic::transport::Server::builder()
            .add_service(NodeServiceServer::with_interceptor(OkNode, AuthInterceptor::new(&t)))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    format!("http://127.0.0.1:{}", port)
}

fn auth_header(id: Uuid, admin: bool) -> String {
    format!("Bearer {}", encode_token(id, admin, "access", SECRET, 900).unwrap())
}

#[sqlx::test(migrations = "crates/panel/migrations")]
async fn full_panel_flow(pool: PgPool) {
    let node_addr = start_node("node-secret").await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let state = AppState { db: pool.clone(), jwt_secret: SECRET.to_string() };
    let app = router(state);

    // 1. Create admin user
    let admin_id = Uuid::new_v4();
    let hash = hash_password("admin-pass").unwrap();
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, is_admin) VALUES ($1, $2, $3, $4)",
    )
    .bind(admin_id).bind("admin@example.com").bind(&hash).bind(true)
    .execute(&pool).await.unwrap();

    // 2. Login
    let login_body = serde_json::json!({
        "email": "admin@example.com", "password": "admin-pass"
    });
    let res = app.clone().oneshot(
        Request::builder().method("POST").uri("/auth/login")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&login_body).unwrap())).unwrap(),
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let tokens: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let access_token = tokens["access_token"].as_str().unwrap().to_string();
    let bearer = format!("Bearer {}", access_token);

    // 3. Create node
    let node_body = serde_json::json!({
        "name": "eu-1", "grpc_addr": node_addr, "token": "node-secret"
    });
    let res = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/nodes")
            .header("authorization", &bearer)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&node_body).unwrap())).unwrap(),
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let node: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let node_id = node["id"].as_str().unwrap();

    // 4. Create + provision server
    let server_body = serde_json::json!({
        "node_id": node_id, "name": "mc-1",
        "image": "itzg/minecraft-server",
        "memory_mb": 1024, "cpu_percent": 100,
        "env": ["EULA=TRUE"]
    });
    let res = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/servers")
            .header("authorization", &bearer)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&server_body).unwrap())).unwrap(),
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let server: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let server_id = server["id"].as_str().unwrap();

    // 5. Get stats
    let res = app.clone().oneshot(
        Request::builder().method("GET")
            .uri(format!("/api/servers/{}/stats", server_id))
            .header("authorization", &bearer)
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let stats: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(stats["memory_bytes"], 256u64);

    // 6. Delete server (stop + delete on node + DB)
    let res = app.oneshot(
        Request::builder().method("DELETE")
            .uri(format!("/api/servers/{}", server_id))
            .header("authorization", &bearer)
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}
```

- [ ] **Step 2: Run (expect compile error on missing imports)**

```bash
DATABASE_URL=postgres://oxy:oxy@localhost/oxy cargo test -p oxy-panel integration 2>&1 | head -20
```

- [ ] **Step 3: Fix lib.rs — verify run() is complete**

The `run()` in `crates/panel/src/lib.rs` should already be complete from Task 1. Verify it matches:
```rust
pub async fn run(config: PanelConfig) -> oxy_core::Result<()> {
    let pool = db::create_pool(&config.database_url)
        .await
        .map_err(|e| OxyError::Config(e.to_string()))?;
    db::run_migrations(&pool)
        .await
        .map_err(|e| OxyError::Config(e.to_string()))?;
    let state = AppState {
        db:         pool,
        jwt_secret: config.jwt_secret,
    };
    tracing::info!(listen = %config.http_listen, "panel starting");
    let listener = tokio::net::TcpListener::bind(&config.http_listen)
        .await
        .map_err(OxyError::Io)?;
    axum::serve(listener, router(state))
        .await
        .map_err(OxyError::Io)
}
```

If any part differs, update it to match exactly.

- [ ] **Step 4: Run all tests**

```bash
DATABASE_URL=postgres://oxy:oxy@localhost/oxy cargo test -p oxy-panel 2>&1
cargo test -p oxy-node 2>&1
cargo test -p oxy-core 2>&1
```
Expected: all oxy-panel tests pass (≥20 tests across unit + integration); all oxy-node tests pass (22); all oxy-core tests pass.

- [ ] **Step 5: Run full workspace build**

```bash
cargo build --workspace 2>&1
```
Expected: compiles cleanly with no errors

- [ ] **Step 6: Commit**

```bash
git add crates/panel/tests/ crates/panel/src/lib.rs
git commit -m "feat(oxy-panel): wire run() and add full integration test"
```
