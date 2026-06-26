# Alpha Fixes: Login Response + SSE Auth + Stats Polling

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Corrigir os três blockers que impedem o frontend de funcionar: login sem email/is_admin, SSE que rejeita ?token=, e stats que é JSON mas o frontend esperava SSE.

**Architecture:** Dois patches no backend (`crates/panel/src/auth.rs` e já existente `servers.rs`) mais uma reescrita do `StatsTab` para fazer polling HTTP simples com `gloo-timers`.

**Tech Stack:** Rust/axum (backend), Leptos 0.8 CSR + gloo-timers (frontend)

## Global Constraints

- Sem comentários no código exceto quando o WHY é não-óbvio
- YAGNI: só o necessário para os três fixes
- Testes com `#[sqlx::test(migrations = "./migrations")]` precisam de `DATABASE_URL`
- Frontend fica em `crates/frontend/` com `[workspace]` vazio no próprio Cargo.toml (isolado do workspace pai)

---

### Task 1: Backend — login retorna email e is_admin

**Files:**
- Modify: `crates/panel/src/auth.rs:148-189`

**Interfaces:**
- Produces: `POST /auth/login` retorna `{ access_token, refresh_token, email, is_admin }`

- [ ] **Step 1: Atualizar `UserRow` para incluir `email`**

Em `crates/panel/src/auth.rs`, trocar:

```rust
#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    password_hash: String,
    is_admin: bool,
}
```

por:

```rust
#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    email: String,
    password_hash: String,
    is_admin: bool,
}
```

- [ ] **Step 2: Atualizar `TokenResponse` para incluir `email` e `is_admin`**

Trocar:

```rust
#[derive(Debug, Serialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
}
```

por:

```rust
#[derive(Debug, Serialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    email: String,
    is_admin: bool,
}
```

- [ ] **Step 3: Atualizar a query SQL e o retorno do handler**

Trocar:

```rust
    let row: Option<UserRow> = sqlx::query_as::<_, UserRow>(
        "SELECT id, password_hash, is_admin FROM users WHERE email = $1",
    )
```

por:

```rust
    let row: Option<UserRow> = sqlx::query_as::<_, UserRow>(
        "SELECT id, email, password_hash, is_admin FROM users WHERE email = $1",
    )
```

E trocar o retorno final do handler:

```rust
    Ok(Json(TokenResponse {
        access_token,
        refresh_token,
    }))
```

por:

```rust
    Ok(Json(TokenResponse {
        access_token,
        refresh_token,
        email: row.email,
        is_admin: row.is_admin,
    }))
```

- [ ] **Step 4: Atualizar o teste existente para verificar os novos campos**

No teste `login_with_valid_credentials_returns_tokens` em `crates/panel/src/auth.rs`, adicionar depois de `assert!(json["refresh_token"].is_string());`:

```rust
        assert_eq!(json["email"].as_str(), Some("admin@example.com"));
        assert_eq!(json["is_admin"].as_bool(), Some(true));
```

- [ ] **Step 5: Rodar testes do panel**

```bash
cd /opt/Oxydactylus
cargo test -p oxy-panel 2>&1 | tail -20
```

Esperado: todos os testes passam (incluindo os novos asserts).

- [ ] **Step 6: Commit**

```bash
git add crates/panel/src/auth.rs
git commit -m "feat(auth): login response includes email and is_admin"
```

---

### Task 2: Backend — AuthUser aceita ?token= como fallback

**Files:**
- Modify: `crates/panel/src/auth.rs:94-118` (impl `FromRequestParts` para `AuthUser`)

**Interfaces:**
- Produces: qualquer endpoint autenticado aceita `?token=<jwt>` quando o header `Authorization` estiver ausente

- [ ] **Step 1: Modificar o extractor `AuthUser` para tentar query param como fallback**

Trocar o corpo de `from_request_parts`:

```rust
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
                PanelError::Unauthorized("missing Authorization header".to_string()).into_response()
            })?;
        let claims = decode_token(token, &state.jwt_secret, "access")
            .map_err(IntoResponse::into_response)?;
        let id = Uuid::parse_str(&claims.sub)
            .map_err(|_| PanelError::Unauthorized("invalid sub".to_string()).into_response())?;
        Ok(AuthUser {
            id,
            is_admin: claims.adm,
        })
    }
```

por:

```rust
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> std::result::Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .or_else(|| {
                parts.uri.query().and_then(|q| {
                    q.split('&').find_map(|kv| kv.strip_prefix("token="))
                })
            })
            .ok_or_else(|| {
                PanelError::Unauthorized("missing token".to_string()).into_response()
            })?;
        let claims = decode_token(token, &state.jwt_secret, "access")
            .map_err(IntoResponse::into_response)?;
        let id = Uuid::parse_str(&claims.sub)
            .map_err(|_| PanelError::Unauthorized("invalid sub".to_string()).into_response())?;
        Ok(AuthUser {
            id,
            is_admin: claims.adm,
        })
    }
```

- [ ] **Step 2: Adicionar teste para auth via query param**

No bloco `#[cfg(test)]` de `crates/panel/src/auth.rs`, adicionar depois do último teste:

```rust
    #[sqlx::test(migrations = "./migrations")]
    async fn auth_via_query_token_param(pool: sqlx::PgPool) {
        let state = make_state(pool.clone()).await;
        let hash = hash_password("pass").unwrap();
        let user_id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (email, password_hash, is_admin) VALUES ($1,$2,$3) RETURNING id",
        )
        .bind("q@example.com")
        .bind(&hash)
        .bind(false)
        .fetch_one(&pool)
        .await
        .unwrap();

        let token = encode_token(user_id, false, "access", SECRET, 900).unwrap();

        // GET /api/me com token na query string
        let app = crate::router(state);
        let req = Request::builder()
            .method("GET")
            .uri(format!("/api/me?token={}", token))
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), axum::http::StatusCode::OK);
    }
```

- [ ] **Step 3: Rodar testes**

```bash
cd /opt/Oxydactylus
cargo test -p oxy-panel 2>&1 | tail -20
```

Esperado: todos os testes passam incluindo `auth_via_query_token_param`.

- [ ] **Step 4: Commit**

```bash
git add crates/panel/src/auth.rs
git commit -m "feat(auth): accept ?token= query param as Bearer fallback for SSE"
```

---

### Task 3: Frontend — StatsTab usa polling HTTP em vez de SSE

O backend `/api/servers/:id/stats` retorna JSON único (não SSE). O `StatsTab` atual
abre um `EventSource` que nunca recebe dados. Fix: polling a cada 3 s com `gloo-timers`.

**Files:**
- Modify: `crates/frontend/Cargo.toml` (adicionar dep)
- Modify: `crates/frontend/src/pages/client/stats_tab.rs`

**Interfaces:**
- Consumes: `GET /api/servers/:id/stats` → `{ memory_bytes, cpu_percent, rx_bytes, tx_bytes }`
- Consumes: `ApiClient::get::<Stats>("/servers/{id}/stats")` definido em `crates/frontend/src/api/client.rs`

- [ ] **Step 1: Adicionar `gloo-timers` ao Cargo.toml do frontend**

Em `crates/frontend/Cargo.toml`, na seção `[dependencies]`, adicionar:

```toml
gloo-timers = { version = "0.3", features = ["futures"] }
```

- [ ] **Step 2: Reescrever `StatsTab` para polling**

Substituir todo o conteúdo de `crates/frontend/src/pages/client/stats_tab.rs` por:

```rust
use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use serde::Deserialize;
use wasm_bindgen_futures::spawn_local;
use crate::api::client::ApiClient;
use crate::state::SessionContext;

#[derive(Clone, Deserialize)]
struct Stats {
    memory_bytes: u64,
    cpu_percent: f32,
    rx_bytes: u64,
    tx_bytes: u64,
}

#[component]
pub fn StatsTab(server_id: String) -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");
    let stats = RwSignal::new(None::<Stats>);
    let running = RwSignal::new(true);

    on_cleanup(move || running.set(false));

    let tok = session.token();
    let id = server_id.clone();
    spawn_local(async move {
        while running.get_untracked() {
            if let Ok(s) = ApiClient::new(tok.clone()).get::<Stats>(&format!("/servers/{}/stats", id)).await {
                stats.set(Some(s));
            }
            TimeoutFuture::new(3_000).await;
        }
    });

    fn fmt_mb(bytes: u64) -> String {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    }

    view! {
        <Show
            when=move || stats.get().is_some()
            fallback=|| view! {
                <p class="text-gray-500 text-sm">"Waiting for stats..."</p>
            }
        >
            {move || stats.get().map(|s| {
                view! {
                    <div class="grid grid-cols-2 gap-4">
                        <div class="bg-white p-4 rounded-lg shadow border">
                            <p class="text-xs text-gray-500 uppercase tracking-wider">"Memory"</p>
                            <p class="text-2xl font-bold mt-1">{fmt_mb(s.memory_bytes)}</p>
                        </div>
                        <div class="bg-white p-4 rounded-lg shadow border">
                            <p class="text-xs text-gray-500 uppercase tracking-wider">"CPU"</p>
                            <p class="text-2xl font-bold mt-1">{format!("{:.1}%", s.cpu_percent)}</p>
                        </div>
                        <div class="bg-white p-4 rounded-lg shadow border">
                            <p class="text-xs text-gray-500 uppercase tracking-wider">"Network RX"</p>
                            <p class="text-2xl font-bold mt-1">{fmt_mb(s.rx_bytes)}</p>
                        </div>
                        <div class="bg-white p-4 rounded-lg shadow border">
                            <p class="text-xs text-gray-500 uppercase tracking-wider">"Network TX"</p>
                            <p class="text-2xl font-bold mt-1">{fmt_mb(s.tx_bytes)}</p>
                        </div>
                    </div>
                }
            })}
        </Show>
    }
}
```

- [ ] **Step 3: Verificar que o frontend compila**

```bash
cd /opt/Oxydactylus/crates/frontend
trunk build 2>&1 | tail -30
```

Esperado: build termina sem erros. Warnings de `unused` são aceitáveis.

- [ ] **Step 4: Commit**

```bash
git add crates/frontend/Cargo.toml crates/frontend/src/pages/client/stats_tab.rs
git commit -m "feat(frontend): stats tab polls HTTP every 3s instead of broken SSE"
```
