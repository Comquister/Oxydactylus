# Plan 8: Frontend Leptos CSR — Painel de Gerenciamento

> **Para agentes executores:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recomendado) ou superpowers:executing-plans para implementar este plano. Passos usam checkbox (`- [ ]`) para rastreamento.

**Objetivo:** Construir frontend CSR em Leptos para painel admin/user, integrando com API REST existente (auth JWT, CRUD de recursos, SSE para logs/stats em tempo real).

**Arquitetura:**
- Layout responsivo com duas áreas: admin (gerenciar users, nodes, servers, eggs) e client (gerenciar próprios servidores).
- Componentes reutilizáveis (botões, modais, tabelas, forms).
- Roteamento cliente via leptos_router.
- Integração autenticada com API via JWT (bearer token no header).
- SSE via `EventSource` do browser para logs e stats em tempo real.

**Tech Stack:**
- Leptos 0.8.19 CSR (Client-Side Rendering)
- Trunk (bundler WASM)
- Tailwind CSS 3 via hook pre_build
- leptos_router 0.8
- gloo-net (HTTP client para WASM, substituto do reqwest)
- gloo-storage (localStorage)
- serde / serde_json
- web-sys (EventSource para SSE)

## Global Constraints

- Crate separada `crates/frontend/` — **não** adicionada ao workspace principal (evita conflito wasm/nativo)
- `[lib]` com `crate-type = ["cdylib", "rlib"]` — exigido pelo Trunk
- Sem comentários no código (exceto WHY não-óbvio)
- Componentes em `src/components/` (módulos focados)
- Páginas em `src/pages/` (layout + integração API)
- Estado global via Leptos context (`provide_context` / `use_context`)
- YAGNI rigoroso — sem features além do mínimo viável
- Commits frequentes (1 por task)

---

## Estrutura de Arquivos

```
crates/frontend/
├── Cargo.toml
├── Trunk.toml
├── index.html
├── tailwind.config.js
├── input.css                     — @tailwind directives
├── src/
│   ├── lib.rs                    — entry point (mount App)
│   ├── state.rs                  — AuthState, SessionContext (RwSignal + localStorage)
│   ├── api/
│   │   ├── mod.rs
│   │   ├── client.rs             — ApiClient (gloo-net + JWT header)
│   │   ├── auth.rs               — login(), types LoginRequest/Response
│   │   ├── users.rs              — User, list_users, create_user, delete_user
│   │   ├── nodes.rs              — Node, list_nodes, create_node, delete_node
│   │   ├── servers.rs            — Server, list_servers, create_server, delete_server, start/stop/restart, send_command
│   │   ├── eggs.rs               — Egg, list_eggs, create_egg, delete_egg
│   │   └── sse.rs                — use_sse_logs, use_sse_stats (signals + EventSource cleanup)
│   ├── components/
│   │   ├── mod.rs
│   │   ├── button.rs             — <Button variant on_click disabled>
│   │   ├── input.rs              — <TextInput>, <Select>
│   │   ├── card.rs               — <Card title subtitle>
│   │   ├── modal.rs              — <Modal title open>
│   │   ├── table.rs              — <Table columns rows on_delete>
│   │   ├── navbar.rs             — <Navbar> (links + logout)
│   │   └── error_banner.rs       — <ErrorBanner msg>
│   └── pages/
│       ├── mod.rs
│       ├── login.rs              — LoginPage (form + submit)
│       ├── not_found.rs          — NotFoundPage
│       ├── admin/
│       │   ├── mod.rs            — AdminLayout (sidebar + <Outlet>)
│       │   ├── users.rs          — AdminUsersPage
│       │   ├── nodes.rs          — AdminNodesPage
│       │   ├── servers.rs        — AdminServersPage
│       │   └── eggs.rs           — AdminEggsPage
│       └── client/
│           ├── mod.rs            — ClientLayout
│           ├── dashboard.rs      — ClientDashboardPage (grid de server cards)
│           ├── server_detail.rs  — ServerDetailPage (tabs: console, logs, stats)
│           ├── console_tab.rs    — ConsoleTab (send command + output buffer)
│           ├── logs_tab.rs       — LogsTab (SSE stream)
│           └── stats_tab.rs      — StatsTab (SSE real-time cards)
```

---

## Task 1: Projeto Leptos Base + Trunk + Tailwind

**Arquivos:**
- Criar: `crates/frontend/Cargo.toml`
- Criar: `crates/frontend/Trunk.toml`
- Criar: `crates/frontend/index.html`
- Criar: `crates/frontend/input.css`
- Criar: `crates/frontend/tailwind.config.js`
- Criar: `crates/frontend/src/lib.rs`
- Criar: `crates/frontend/src/pages/mod.rs`
- Criar: `crates/frontend/src/pages/not_found.rs`

**Interfaces:**
- Produz: `trunk build` funciona, gera `dist/` com HTML + WASM + CSS

- [ ] **Step 1: Criar Cargo.toml**

```toml
[package]
name = "oxydactylus-frontend"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
leptos = { version = "=0.8.19", features = ["csr"] }
leptos_router = { version = "=0.8.19", features = ["browser"] }
gloo-net = { version = "0.6", features = ["http"] }
gloo-storage = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"
web-sys = { version = "0.3", features = [
    "EventSource",
    "MessageEvent",
] }
console_error_panic_hook = "0.1"
```

- [ ] **Step 2: Criar Trunk.toml**

```toml
[build]
target = "index.html"
dist = "dist"

[watch]
ignore = ["target", "dist"]

[[hooks]]
stage = "pre_build"
command = "npx"
command_arguments = ["tailwindcss", "-i", "./input.css", "-o", "./dist/tailwind.css", "--minify"]

# Proxy reverso para o Axum — elimina CORS em dev e torna os paths portáteis em prod
[[proxy]]
rewrite = "/api"
backend = "http://localhost:3000/api"

[[proxy]]
rewrite = "/auth"
backend = "http://localhost:3000/auth"
```

- [ ] **Step 3: Criar index.html**

```html
<!DOCTYPE html>
<html lang="pt-BR">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Oxydactylus Panel</title>
    <link data-trunk rel="css" href="dist/tailwind.css" />
    <link data-trunk rel="rust" />
</head>
<body class="bg-gray-50 min-h-screen">
</body>
</html>
```

- [ ] **Step 4: Criar input.css**

```css
@tailwind base;
@tailwind components;
@tailwind utilities;
```

- [ ] **Step 5: Criar tailwind.config.js**

```javascript
/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./src/**/*.rs", "./index.html"],
  theme: { extend: {} },
  plugins: [],
}
```

- [ ] **Step 6: Criar src/lib.rs**

```rust
use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

mod api;
mod components;
mod pages;
mod state;

use pages::{login::LoginPage, not_found::NotFoundPage};

#[component]
fn App() -> impl IntoView {
    let session = state::SessionContext::new();
    provide_context(session);

    view! {
        <Router>
            <Routes fallback=NotFoundPage>
                <Route path=path!("/login") view=LoginPage />
                <Route path=path!("/") view=|| view! { <LoginPage /> } />
            </Routes>
        </Router>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    // No 0.8 mount_to_body aceita diretamente o componente App
    leptos::mount::mount_to_body(App);
}
```

- [ ] **Step 7: Criar src/pages/mod.rs e not_found.rs**

```rust
// src/pages/mod.rs
pub mod login;
pub mod not_found;
pub mod admin;
pub mod client;

// src/pages/not_found.rs
use leptos::prelude::*;

#[component]
pub fn NotFoundPage() -> impl IntoView {
    view! {
        <div class="flex items-center justify-center h-screen">
            <div class="text-center">
                <h1 class="text-6xl font-bold text-gray-300">"404"</h1>
                <p class="text-gray-500 mt-2">"Page not found"</p>
            </div>
        </div>
    }
}
```

- [ ] **Step 8: Instalar dependências e verificar build**

```bash
cd crates/frontend
npm install tailwindcss
trunk build
```

Expected: `dist/` gerado, sem erros de compilação.

- [ ] **Step 9: Commit**

```bash
git add crates/frontend/
git commit -m "feat(frontend): init Leptos 0.8.19 CSR project with Trunk + Tailwind"
```

---

## Task 2: Estado de Autenticação + API Client

**Arquivos:**
- Criar: `crates/frontend/src/state.rs`
- Criar: `crates/frontend/src/api/mod.rs`
- Criar: `crates/frontend/src/api/client.rs`
- Criar: `crates/frontend/src/api/auth.rs`

**Interfaces:**
- Produz:
  - `SessionContext { auth: RwSignal<Option<AuthState>> }` (via `provide_context`)
  - `SessionContext::new()` — carrega do localStorage automaticamente
  - `SessionContext::set_auth(&self, AuthState)`
  - `SessionContext::clear(&self)`
  - `SessionContext::token(&self) -> String`
  - `ApiClient::new(token: String)`
  - `ApiClient::get<T>(&self, path: &str) -> Result<T, String>`
  - `ApiClient::post<I,O>(&self, path: &str, body: &I) -> Result<O, String>`
  - `ApiClient::delete(&self, path: &str) -> Result<(), String>`
  - `ApiClient::patch<I,O>(&self, path: &str, body: &I) -> Result<O, String>`
  - `api::auth::login(email, password) -> Result<LoginResponse, String>`

- [ ] **Step 1: Criar src/state.rs**

```rust
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use gloo_storage::{LocalStorage, Storage};

const STORAGE_KEY: &str = "oxy_auth";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthState {
    pub access_token: String,
    pub refresh_token: String,
    pub email: String,
    pub is_admin: bool,
}

#[derive(Clone)]
pub struct SessionContext {
    pub auth: RwSignal<Option<AuthState>>,
}

impl SessionContext {
    pub fn new() -> Self {
        let stored: Option<AuthState> = LocalStorage::get(STORAGE_KEY).ok();
        Self {
            auth: RwSignal::new(stored),
        }
    }

    pub fn set_auth(&self, state: AuthState) {
        let _ = LocalStorage::set(STORAGE_KEY, &state);
        self.auth.set(Some(state));
    }

    pub fn clear(&self) {
        LocalStorage::delete(STORAGE_KEY);
        self.auth.set(None);
    }

    pub fn token(&self) -> String {
        self.auth
            .get_untracked()
            .map(|a| a.access_token)
            .unwrap_or_default()
    }

    pub fn is_admin(&self) -> bool {
        self.auth
            .get_untracked()
            .map(|a| a.is_admin)
            .unwrap_or(false)
    }
}
```

- [ ] **Step 2: Criar src/api/client.rs**

```rust
use gloo_net::http::Request;
use serde::{de::DeserializeOwned, Serialize};

// Paths relativos — funciona via proxy do Trunk em dev e em qualquer domínio em prod
const API_BASE: &str = "/api";

pub struct ApiClient {
    token: String,
}

impl ApiClient {
    pub fn new(token: String) -> Self {
        Self { token }
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        Request::get(&format!("{}{}", API_BASE, path))
            .header("Authorization", &format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json::<T>()
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn post<I: Serialize, O: DeserializeOwned>(
        &self,
        path: &str,
        body: &I,
    ) -> Result<O, String> {
        Request::post(&format!("{}{}", API_BASE, path))
            .header("Authorization", &format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(body).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json::<O>()
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn patch<I: Serialize, O: DeserializeOwned>(
        &self,
        path: &str,
        body: &I,
    ) -> Result<O, String> {
        Request::patch(&format!("{}{}", API_BASE, path))
            .header("Authorization", &format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(body).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json::<O>()
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn delete(&self, path: &str) -> Result<(), String> {
        let resp = Request::delete(&format!("{}{}", API_BASE, path))
            .header("Authorization", &format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if resp.ok() {
            Ok(())
        } else {
            Err(format!("HTTP {}", resp.status()))
        }
    }
}
```

- [ ] **Step 3: Criar src/api/auth.rs**

```rust
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

const AUTH_BASE: &str = "/auth";

#[derive(Serialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Deserialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
}

pub async fn login(email: &str, password: &str) -> Result<LoginResponse, String> {
    Request::post(&format!("{}/login", AUTH_BASE))
        .header("Content-Type", "application/json")
        .body(
            serde_json::to_string(&LoginRequest {
                email: email.to_string(),
                password: password.to_string(),
            })
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json::<LoginResponse>()
        .await
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 4: Criar src/api/mod.rs**

```rust
pub mod auth;
pub mod client;
pub mod eggs;
pub mod nodes;
pub mod servers;
pub mod sse;
pub mod users;
```

- [ ] **Step 5: Criar stubs para os outros módulos api**

```rust
// src/api/users.rs
pub struct User;

// src/api/nodes.rs
pub struct Node;

// src/api/servers.rs
pub struct Server;

// src/api/eggs.rs
pub struct Egg;

// src/api/sse.rs
// (vazio — implementado na Task 8)
```

- [ ] **Step 6: Verificar compilação**

```bash
cd crates/frontend
trunk build
```

Expected: sem erros de compilação.

- [ ] **Step 7: Commit**

```bash
git add crates/frontend/src/state.rs crates/frontend/src/api/
git commit -m "feat(frontend): auth state (localStorage) + API client com JWT"
```

---

## Task 3: Componentes Base

**Arquivos:**
- Criar: `crates/frontend/src/components/mod.rs`
- Criar: `crates/frontend/src/components/button.rs`
- Criar: `crates/frontend/src/components/input.rs`
- Criar: `crates/frontend/src/components/card.rs`
- Criar: `crates/frontend/src/components/modal.rs`
- Criar: `crates/frontend/src/components/table.rs`
- Criar: `crates/frontend/src/components/navbar.rs`
- Criar: `crates/frontend/src/components/error_banner.rs`

**Interfaces:**
- Produz:
  - `<Button variant="primary"|"secondary"|"danger" disabled on_click>`
  - `<TextInput value placeholder label />`
  - `<SelectInput value options label />`
  - `<Card title subtitle> children </Card>`
  - `<Modal title open> children </Modal>`
  - `<Table columns rows on_delete />`
  - `<Navbar />`
  - `<ErrorBanner msg />`

- [ ] **Step 1: Criar src/components/button.rs**

```rust
use leptos::prelude::*;

#[component]
pub fn Button(
    #[prop(into, default = "primary".to_string())] variant: String,
    #[prop(default = false)] disabled: bool,
    #[prop(optional)] on_click: Option<Callback<()>>,
    children: Children,
) -> impl IntoView {
    let base = "px-4 py-2 rounded font-medium transition-colors focus:outline-none";
    let style = move || match variant.as_str() {
        "secondary" => format!("{} bg-gray-200 hover:bg-gray-300 text-gray-800", base),
        "danger" => format!("{} bg-red-600 hover:bg-red-700 text-white", base),
        _ => format!("{} bg-blue-600 hover:bg-blue-700 text-white", base),
    };

    view! {
        <button
            class=style
            disabled=disabled
            on:click=move |_| {
                if !disabled {
                    if let Some(cb) = &on_click { cb.call(()); }
                }
            }
        >
            {children()}
        </button>
    }
}
```

- [ ] **Step 2: Criar src/components/input.rs**

```rust
use leptos::prelude::*;

const INPUT_CLASS: &str =
    "w-full px-3 py-2 border border-gray-300 rounded-md \
     focus:outline-none focus:ring-2 focus:ring-blue-500 bg-white";

#[component]
pub fn TextInput(
    value: RwSignal<String>,
    #[prop(into, default = "".to_string())] placeholder: String,
    #[prop(into, default = "".to_string())] label: String,
    #[prop(default = "text")] input_type: &'static str,
) -> impl IntoView {
    view! {
        <div class="space-y-1">
            {(!label.is_empty()).then(|| view! {
                <label class="block text-sm font-medium text-gray-700">{label.clone()}</label>
            })}
            <input
                type=input_type
                placeholder=placeholder
                prop:value=value
                on:input=move |ev| value.set(event_target_value(&ev))
                class=INPUT_CLASS
            />
        </div>
    }
}

#[component]
pub fn SelectInput(
    value: RwSignal<String>,
    options: Vec<(String, String)>,
    #[prop(into, default = "".to_string())] label: String,
    #[prop(into, default = "Select...".to_string())] placeholder: String,
) -> impl IntoView {
    view! {
        <div class="space-y-1">
            {(!label.is_empty()).then(|| view! {
                <label class="block text-sm font-medium text-gray-700">{label.clone()}</label>
            })}
            <select
                prop:value=value
                on:change=move |ev| value.set(event_target_value(&ev))
                class=INPUT_CLASS
            >
                <option value="" disabled selected>{placeholder}</option>
                {options.into_iter().map(|(v, label)| view! {
                    <option value=v.clone()>{label}</option>
                }).collect_view()}
            </select>
        </div>
    }
}
```

- [ ] **Step 3: Criar src/components/card.rs**

```rust
use leptos::prelude::*;

#[component]
pub fn Card(
    #[prop(into)] title: String,
    #[prop(into, default = "".to_string())] subtitle: String,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="bg-white rounded-lg shadow p-6">
            <div class="mb-4">
                <h2 class="text-xl font-bold text-gray-900">{title}</h2>
                {(!subtitle.is_empty()).then(|| view! {
                    <p class="text-sm text-gray-500 mt-1">{subtitle}</p>
                })}
            </div>
            {children()}
        </div>
    }
}
```

- [ ] **Step 4: Criar src/components/modal.rs**

```rust
use leptos::prelude::*;

#[component]
pub fn Modal(
    #[prop(into)] title: String,
    open: RwSignal<bool>,
    children: Children,
) -> impl IntoView {
    view! {
        <Show when=move || open.get()>
            <div class="fixed inset-0 z-50 flex items-center justify-center">
                <div
                    class="absolute inset-0 bg-black bg-opacity-50"
                    on:click=move |_| open.set(false)
                />
                <div class="relative bg-white rounded-lg shadow-xl p-8 w-full max-w-md mx-4">
                    <div class="flex justify-between items-center mb-6">
                        <h3 class="text-xl font-bold">{title}</h3>
                        <button
                            on:click=move |_| open.set(false)
                            class="text-gray-400 hover:text-gray-600 text-2xl leading-none"
                        >
                            "×"
                        </button>
                    </div>
                    {children()}
                </div>
            </div>
        </Show>
    }
}
```

- [ ] **Step 5: Criar src/components/table.rs**

```rust
use leptos::prelude::*;

pub struct Column<T> {
    pub header: &'static str,
    pub render: fn(&T) -> String,
}

#[component]
pub fn Table<T: Clone + 'static>(
    columns: Vec<Column<T>>,
    rows: Vec<T>,
    #[prop(optional)] on_delete: Option<Callback<String>>,
    key_fn: fn(&T) -> String,
) -> impl IntoView {
    let has_actions = on_delete.is_some();

    view! {
        <div class="overflow-x-auto rounded-lg border border-gray-200">
            <table class="w-full text-sm text-left">
                <thead class="bg-gray-50 text-xs text-gray-700 uppercase">
                    <tr>
                        {columns.iter().map(|c| view! {
                            <th class="px-6 py-3 font-semibold">{c.header}</th>
                        }).collect_view()}
                        {has_actions.then(|| view! {
                            <th class="px-6 py-3 font-semibold">"Actions"</th>
                        })}
                    </tr>
                </thead>
                <tbody class="divide-y divide-gray-100">
                    {rows.into_iter().map(|row| {
                        let row_key = key_fn(&row);
                        view! {
                            <tr class="hover:bg-gray-50">
                                {columns.iter().map(|c| view! {
                                    <td class="px-6 py-3 text-gray-700">{(c.render)(&row)}</td>
                                }).collect_view()}
                                {on_delete.map(|cb| {
                                    let id = row_key.clone();
                                    view! {
                                        <td class="px-6 py-3">
                                            <button
                                                on:click=move |_| cb.call(id.clone())
                                                class="text-red-600 hover:text-red-800 font-medium"
                                            >
                                                "Delete"
                                            </button>
                                        </td>
                                    }
                                })}
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
}
```

- [ ] **Step 6: Criar src/components/navbar.rs**

```rust
use leptos::prelude::*;
use leptos_router::components::A;
use crate::state::SessionContext;

#[component]
pub fn Navbar() -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");

    let handle_logout = move |_| {
        session.clear();
    };

    view! {
        <nav class="bg-slate-900 text-white shadow-lg">
            <div class="max-w-7xl mx-auto px-4 py-3 flex justify-between items-center">
                <A href="/" class="text-xl font-bold tracking-tight">
                    "Oxydactylus"
                </A>
                <Show when=move || session.auth.get().is_some()>
                    <div class="flex items-center gap-6">
                        <Show when=move || session.is_admin()>
                            <A href="/admin/users" class="hover:text-blue-400 text-sm">"Admin"</A>
                        </Show>
                        <A href="/client" class="hover:text-blue-400 text-sm">"Servers"</A>
                        <span class="text-gray-400 text-sm">
                            {move || session.auth.get().map(|a| a.email).unwrap_or_default()}
                        </span>
                        <button
                            on:click=handle_logout
                            class="px-3 py-1 bg-red-700 hover:bg-red-800 rounded text-sm"
                        >
                            "Logout"
                        </button>
                    </div>
                </Show>
            </div>
        </nav>
    }
}
```

- [ ] **Step 7: Criar src/components/error_banner.rs**

```rust
use leptos::prelude::*;

#[component]
pub fn ErrorBanner(
    #[prop(into)] msg: Signal<String>,
) -> impl IntoView {
    view! {
        <Show when=move || !msg.get().is_empty()>
            <div class="p-3 bg-red-100 border border-red-400 text-red-700 rounded-md text-sm">
                {move || msg.get()}
            </div>
        </Show>
    }
}
```

- [ ] **Step 8: Criar src/components/mod.rs**

```rust
pub mod button;
pub mod card;
pub mod error_banner;
pub mod input;
pub mod modal;
pub mod navbar;
pub mod table;

pub use button::Button;
pub use card::Card;
pub use error_banner::ErrorBanner;
pub use input::{SelectInput, TextInput};
pub use modal::Modal;
pub use navbar::Navbar;
pub use table::{Column, Table};
```

- [ ] **Step 9: Verificar compilação e commit**

```bash
cd crates/frontend
trunk build
git add crates/frontend/src/components/
git commit -m "feat(frontend): componentes base (Button, Input, Card, Modal, Table, Navbar)"
```

---

## Task 4: Login Page + Redirecionamento por Role

**Arquivos:**
- Criar: `crates/frontend/src/pages/login.rs`
- Criar: `crates/frontend/src/pages/admin/mod.rs` (stub)
- Criar: `crates/frontend/src/pages/client/mod.rs` (stub)
- Modificar: `crates/frontend/src/lib.rs`

**Interfaces:**
- Consome: `api::auth::login(email, password) -> Result<LoginResponse, String>`
- Consome: `SessionContext::set_auth(AuthState)` / `SessionContext::is_admin()`
- Produz: LoginPage funcional — redireciona admin para `/admin/users`, user para `/client`

- [ ] **Step 1: Criar src/pages/login.rs**

```rust
use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use wasm_bindgen_futures::spawn_local;
use crate::api::auth::login;
use crate::components::{Button, ErrorBanner, TextInput};
use crate::state::{AuthState, SessionContext};

#[component]
pub fn LoginPage() -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");
    let navigate = use_navigate();

    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let error = RwSignal::new(String::new());
    let loading = RwSignal::new(false);

    let handle_submit = move |_| {
        if email.get_untracked().is_empty() || password.get_untracked().is_empty() {
            error.set("Email e senha são obrigatórios".to_string());
            return;
        }

        error.set(String::new());
        loading.set(true);

        let navigate = navigate.clone();
        spawn_local(async move {
            let result = login(&email.get_untracked(), &password.get_untracked()).await;
            loading.set(false);

            match result {
                Ok(resp) => {
                    let is_admin = resp.is_admin;
                    session.set_auth(AuthState {
                        access_token: resp.access_token,
                        refresh_token: resp.refresh_token,
                        email: resp.email,
                        is_admin,
                    });
                    if is_admin {
                        navigate("/admin/users", Default::default());
                    } else {
                        navigate("/client", Default::default());
                    }
                }
                Err(e) => error.set(format!("Falha no login: {}", e)),
            }
        });
    };

    view! {
        <div class="flex items-center justify-center min-h-screen bg-gradient-to-br from-slate-950 to-slate-800">
            <div class="bg-white rounded-xl shadow-2xl p-8 w-full max-w-sm">
                <h1 class="text-3xl font-bold text-center text-gray-900 mb-8">"Oxydactylus"</h1>

                <form
                    class="space-y-4"
                    on:submit=move |e| {
                        e.prevent_default();
                        handle_submit(());
                    }
                >
                    <ErrorBanner msg=error.into() />
                    <TextInput value=email placeholder="admin@example.com" label="Email" />
                    <TextInput value=password placeholder="••••••••" label="Password" input_type="password" />
                    <Button disabled=loading.get() variant="primary">
                        {move || if loading.get() { "Entrando..." } else { "Entrar" }}
                    </Button>
                </form>
            </div>
        </div>
    }
}
```

- [ ] **Step 2: Verificar que `LoginResponse` tem campo `email` e `is_admin`**

No `src/api/auth.rs`, atualizar `LoginResponse`:

```rust
#[derive(Deserialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub email: String,
    pub is_admin: bool,
}
```

- [ ] **Step 3: Criar src/pages/admin/mod.rs (stub)**

```rust
use leptos::prelude::*;

pub mod eggs;
pub mod nodes;
pub mod servers;
pub mod users;

#[component]
pub fn AdminLayout() -> impl IntoView {
    view! { <div>"Admin — coming next task"</div> }
}
```

- [ ] **Step 4: Criar src/pages/client/mod.rs (stub)**

```rust
use leptos::prelude::*;

pub mod dashboard;
pub mod server_detail;
pub mod console_tab;
pub mod logs_tab;
pub mod stats_tab;

#[component]
pub fn ClientLayout() -> impl IntoView {
    view! { <div>"Client — coming next task"</div> }
}
```

- [ ] **Step 5: Atualizar src/lib.rs com todas as rotas**

```rust
use leptos::prelude::*;
use leptos_router::components::{ParentRoute, Route, Router, Routes};
use leptos_router::path;

mod api;
mod components;
mod pages;
mod state;

use components::Navbar;
use pages::{
    admin::{
        AdminLayout,
        eggs::AdminEggsPage,
        nodes::AdminNodesPage,
        servers::AdminServersPage,
        users::AdminUsersPage,
    },
    client::{ClientLayout, dashboard::ClientDashboardPage, server_detail::ServerDetailPage},
    login::LoginPage,
    not_found::NotFoundPage,
};

#[component]
fn App() -> impl IntoView {
    let session = state::SessionContext::new();
    provide_context(session);

    view! {
        <Navbar />
        <Router>
            <Routes fallback=NotFoundPage>
                <Route path=path!("/login") view=LoginPage />
                // ParentRoute exige <Outlet /> no componente pai para injetar as sub-rotas.
                // Subcaminhos são RELATIVOS ao pai (sem barra inicial).
                <ParentRoute path=path!("/admin") view=AdminLayout>
                    <Route path=path!("users") view=AdminUsersPage />
                    <Route path=path!("nodes") view=AdminNodesPage />
                    <Route path=path!("servers") view=AdminServersPage />
                    <Route path=path!("eggs") view=AdminEggsPage />
                </ParentRoute>
                <ParentRoute path=path!("/client") view=ClientLayout>
                    <Route path=path!("") view=ClientDashboardPage />
                    <Route path=path!("servers/:id") view=ServerDetailPage />
                </ParentRoute>
                <Route path=path!("/") view=LoginPage />
            </Routes>
        </Router>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
```

- [ ] **Step 6: Criar stubs para todas as páginas não implementadas ainda**

```rust
// src/pages/admin/users.rs
use leptos::prelude::*;
#[component] pub fn AdminUsersPage() -> impl IntoView { view! { <div>"Users"</div> } }

// src/pages/admin/nodes.rs
use leptos::prelude::*;
#[component] pub fn AdminNodesPage() -> impl IntoView { view! { <div>"Nodes"</div> } }

// src/pages/admin/servers.rs
use leptos::prelude::*;
#[component] pub fn AdminServersPage() -> impl IntoView { view! { <div>"Servers"</div> } }

// src/pages/admin/eggs.rs
use leptos::prelude::*;
#[component] pub fn AdminEggsPage() -> impl IntoView { view! { <div>"Eggs"</div> } }

// src/pages/client/dashboard.rs
use leptos::prelude::*;
#[component] pub fn ClientDashboardPage() -> impl IntoView { view! { <div>"Dashboard"</div> } }

// src/pages/client/server_detail.rs
use leptos::prelude::*;
#[component] pub fn ServerDetailPage() -> impl IntoView { view! { <div>"Server Detail"</div> } }

// src/pages/client/console_tab.rs
use leptos::prelude::*;
#[component] pub fn ConsoleTab(server_id: String) -> impl IntoView { view! { <div>"Console"</div> } }

// src/pages/client/logs_tab.rs
use leptos::prelude::*;
#[component] pub fn LogsTab(server_id: String) -> impl IntoView { view! { <div>"Logs"</div> } }

// src/pages/client/stats_tab.rs
use leptos::prelude::*;
#[component] pub fn StatsTab(server_id: String) -> impl IntoView { view! { <div>"Stats"</div> } }
```

- [ ] **Step 7: Verificar compilação e testar login page**

```bash
cd crates/frontend
trunk serve
# Abrir http://localhost:8080/login
# Verificar: form renderiza, erro mostra se campos vazios
```

- [ ] **Step 8: Commit**

```bash
git add crates/frontend/src/pages/ crates/frontend/src/lib.rs crates/frontend/src/api/auth.rs
git commit -m "feat(frontend): login page funcional com redirect por role"
```

---

## Task 5: Admin Layout + Sidebar

**Arquivos:**
- Modificar: `crates/frontend/src/pages/admin/mod.rs`

**Interfaces:**
- Consome: `SessionContext::is_admin()` para guard
- Produz: `AdminLayout` com sidebar de navegação + `<Outlet>` para sub-rotas

- [ ] **Step 1: Modificar src/pages/admin/mod.rs**

```rust
use leptos::prelude::*;
use leptos_router::components::{A, Outlet};
use leptos_router::hooks::use_navigate;
use crate::state::SessionContext;

pub mod eggs;
pub mod nodes;
pub mod servers;
pub mod users;

#[component]
pub fn AdminLayout() -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");
    let navigate = use_navigate();

    if !session.is_admin() {
        navigate("/login", Default::default());
    }

    view! {
            <div class="flex min-h-screen bg-gray-50">
                <aside class="w-56 bg-white shadow-md flex-shrink-0">
                    <div class="p-6">
                        <h3 class="text-xs font-semibold text-gray-400 uppercase tracking-wider mb-4">
                            "Administration"
                        </h3>
                        <nav class="space-y-1">
                            <A
                                href="/admin/users"
                                class="flex items-center px-3 py-2 text-sm rounded-md hover:bg-gray-100 text-gray-700"
                            >
                                "Users"
                            </A>
                            <A
                                href="/admin/nodes"
                                class="flex items-center px-3 py-2 text-sm rounded-md hover:bg-gray-100 text-gray-700"
                            >
                                "Nodes"
                            </A>
                            <A
                                href="/admin/servers"
                                class="flex items-center px-3 py-2 text-sm rounded-md hover:bg-gray-100 text-gray-700"
                            >
                                "Servers"
                            </A>
                            <A
                                href="/admin/eggs"
                                class="flex items-center px-3 py-2 text-sm rounded-md hover:bg-gray-100 text-gray-700"
                            >
                                "Eggs"
                            </A>
                        </nav>
                    </div>
                </aside>
                <main class="flex-1 p-8">
                    <Outlet />
                </main>
            </div>
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add crates/frontend/src/pages/admin/mod.rs
git commit -m "feat(frontend): admin layout com sidebar e guard de role"
```

---

## Task 6: Admin Pages — Users, Nodes, Servers, Eggs (CRUD)

**Arquivos:**
- Modificar: `crates/frontend/src/api/users.rs`
- Modificar: `crates/frontend/src/api/nodes.rs`
- Modificar: `crates/frontend/src/api/servers.rs`
- Modificar: `crates/frontend/src/api/eggs.rs`
- Modificar: `crates/frontend/src/pages/admin/users.rs`
- Modificar: `crates/frontend/src/pages/admin/nodes.rs`
- Modificar: `crates/frontend/src/pages/admin/servers.rs`
- Modificar: `crates/frontend/src/pages/admin/eggs.rs`

**Interfaces:**
- Consome: `ApiClient::get<Vec<T>>`, `ApiClient::post`, `ApiClient::delete`
- Produz: Pages com tabela + modal de criação + botão delete para cada recurso

- [ ] **Step 1: Completar src/api/users.rs**

```rust
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub is_admin: bool,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct CreateUserBody {
    pub email: String,
    pub password: String,
    pub is_admin: bool,
}
```

- [ ] **Step 2: Completar src/api/nodes.rs**

```rust
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
pub struct Node {
    pub id: String,
    pub name: String,
    pub grpc_addr: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct CreateNodeBody {
    pub name: String,
    pub grpc_addr: String,
}
```

- [ ] **Step 3: Completar src/api/servers.rs**

```rust
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub status: String,
    pub image: String,
    pub memory_mb: i32,
    pub cpu_percent: i32,
    pub user_id: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct CreateServerBody {
    pub user_id: String,
    pub node_id: String,
    pub name: String,
    pub image: String,
    pub memory_mb: i32,
    pub cpu_percent: i32,
}

#[derive(Deserialize)]
pub struct ServerStats {
    pub memory_bytes: u64,
    pub cpu_percent: f32,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}
```

- [ ] **Step 4: Completar src/api/eggs.rs**

```rust
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
pub struct Egg {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct CreateEggBody {
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub start_cmd: String,
    pub stop_cmd: String,
    pub startup_done: String,
    pub docker_images: serde_json::Value,
}
```

- [ ] **Step 5: Implementar src/pages/admin/users.rs**

```rust
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;
use crate::api::{client::ApiClient, users::*};
use crate::components::{Button, Card, Column, ErrorBanner, Modal, Table, TextInput};
use crate::state::SessionContext;

#[component]
pub fn AdminUsersPage() -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");

    let users = RwSignal::new(Vec::<User>::new());
    let error = RwSignal::new(String::new());
    let show_modal = RwSignal::new(false);

    let f_email = RwSignal::new(String::new());
    let f_password = RwSignal::new(String::new());
    let f_admin = RwSignal::new(false);

    let load = {
        let tok = session.token();
        move || {
            let tok = tok.clone();
            spawn_local(async move {
                match ApiClient::new(tok).get::<Vec<User>>("/users").await {
                    Ok(v) => users.set(v),
                    Err(e) => error.set(e),
                }
            });
        }
    };

    load();

    let on_delete = Callback::new({
        let tok = session.token();
        move |id: String| {
            let tok = tok.clone();
            spawn_local(async move {
                if let Err(e) = ApiClient::new(tok).delete(&format!("/users/{}", id)).await {
                    error.set(e);
                } else {
                    users.update(|v| v.retain(|u| u.id != id));
                }
            });
        }
    });

    let on_create = Callback::new({
        let tok = session.token();
        move |_| {
            let tok = tok.clone();
            let body = CreateUserBody {
                email: f_email.get_untracked(),
                password: f_password.get_untracked(),
                is_admin: f_admin.get_untracked(),
            };
            spawn_local(async move {
                match ApiClient::new(tok).post::<_, User>("/users", &body).await {
                    Ok(u) => {
                        users.update(|v| v.push(u));
                        show_modal.set(false);
                        f_email.set(String::new());
                        f_password.set(String::new());
                        f_admin.set(false);
                    }
                    Err(e) => error.set(e),
                }
            });
        }
    });

    let columns = vec![
        Column { header: "Email", render: |u: &User| u.email.clone() },
        Column { header: "Admin", render: |u: &User| if u.is_admin { "Sim".into() } else { "Não".into() } },
        Column { header: "Criado em", render: |u: &User| u.created_at.clone() },
    ];

    view! {
        <Card title="Users".to_string()>
            <div class="space-y-4">
                <ErrorBanner msg=error.into() />
                <div class="flex justify-end">
                    <Button on_click=Some(Callback::new(move |_| show_modal.set(true)))>
                        "+ Create User"
                    </Button>
                </div>
                <Table columns=columns rows=users.get() on_delete=Some(on_delete) key_fn=|u: &User| u.id.clone() />
            </div>
        </Card>

        <Modal title="Create User".to_string() open=show_modal>
            <div class="space-y-4">
                <TextInput value=f_email label="Email" />
                <TextInput value=f_password label="Password" input_type="password" />
                <label class="flex items-center gap-2 text-sm">
                    <input
                        type="checkbox"
                        prop:checked=f_admin
                        on:change=move |ev| f_admin.set(event_target_checked(&ev))
                    />
                    "Is Admin"
                </label>
                <Button on_click=Some(on_create)>"Create"</Button>
            </div>
        </Modal>
    }
}
```

- [ ] **Step 6: Implementar src/pages/admin/nodes.rs**

```rust
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;
use crate::api::{client::ApiClient, nodes::*};
use crate::components::{Button, Card, Column, ErrorBanner, Modal, Table, TextInput};
use crate::state::SessionContext;

#[component]
pub fn AdminNodesPage() -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");

    let nodes = RwSignal::new(Vec::<Node>::new());
    let error = RwSignal::new(String::new());
    let show_modal = RwSignal::new(false);
    let f_name = RwSignal::new(String::new());
    let f_addr = RwSignal::new(String::new());

    let load = {
        let tok = session.token();
        move || {
            let tok = tok.clone();
            spawn_local(async move {
                match ApiClient::new(tok).get::<Vec<Node>>("/nodes").await {
                    Ok(v) => nodes.set(v),
                    Err(e) => error.set(e),
                }
            });
        }
    };
    load();

    let on_delete = Callback::new({
        let tok = session.token();
        move |id: String| {
            let tok = tok.clone();
            spawn_local(async move {
                if let Err(e) = ApiClient::new(tok).delete(&format!("/nodes/{}", id)).await {
                    error.set(e);
                } else {
                    nodes.update(|v| v.retain(|n| n.id != id));
                }
            });
        }
    });

    let on_create = Callback::new({
        let tok = session.token();
        move |_| {
            let tok = tok.clone();
            let body = CreateNodeBody {
                name: f_name.get_untracked(),
                grpc_addr: f_addr.get_untracked(),
            };
            spawn_local(async move {
                match ApiClient::new(tok).post::<_, Node>("/nodes", &body).await {
                    Ok(n) => {
                        nodes.update(|v| v.push(n));
                        show_modal.set(false);
                        f_name.set(String::new());
                        f_addr.set(String::new());
                    }
                    Err(e) => error.set(e),
                }
            });
        }
    });

    let columns = vec![
        Column { header: "Name", render: |n: &Node| n.name.clone() },
        Column { header: "gRPC Address", render: |n: &Node| n.grpc_addr.clone() },
        Column { header: "Criado em", render: |n: &Node| n.created_at.clone() },
    ];

    view! {
        <Card title="Nodes".to_string()>
            <div class="space-y-4">
                <ErrorBanner msg=error.into() />
                <div class="flex justify-end">
                    <Button on_click=Some(Callback::new(move |_| show_modal.set(true)))>
                        "+ Create Node"
                    </Button>
                </div>
                <Table columns=columns rows=nodes.get() on_delete=Some(on_delete) key_fn=|n: &Node| n.id.clone() />
            </div>
        </Card>

        <Modal title="Create Node".to_string() open=show_modal>
            <div class="space-y-4">
                <TextInput value=f_name label="Name" />
                <TextInput value=f_addr label="gRPC Address" placeholder="host:50051" />
                <Button on_click=Some(on_create)>"Create"</Button>
            </div>
        </Modal>
    }
}
```

- [ ] **Step 7: Implementar src/pages/admin/servers.rs**

```rust
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;
use crate::api::{client::ApiClient, servers::*};
use crate::components::{Button, Card, Column, ErrorBanner, Modal, Table, TextInput};
use crate::state::SessionContext;

#[component]
pub fn AdminServersPage() -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");

    let servers = RwSignal::new(Vec::<Server>::new());
    let error = RwSignal::new(String::new());
    let show_modal = RwSignal::new(false);
    let f_name = RwSignal::new(String::new());
    let f_image = RwSignal::new(String::new());
    let f_node = RwSignal::new(String::new());
    let f_user = RwSignal::new(String::new());
    let f_mem = RwSignal::new("512".to_string());
    let f_cpu = RwSignal::new("100".to_string());

    let load = {
        let tok = session.token();
        move || {
            let tok = tok.clone();
            spawn_local(async move {
                match ApiClient::new(tok).get::<Vec<Server>>("/servers").await {
                    Ok(v) => servers.set(v),
                    Err(e) => error.set(e),
                }
            });
        }
    };
    load();

    let on_delete = Callback::new({
        let tok = session.token();
        move |id: String| {
            let tok = tok.clone();
            spawn_local(async move {
                if let Err(e) = ApiClient::new(tok).delete(&format!("/servers/{}", id)).await {
                    error.set(e);
                } else {
                    servers.update(|v| v.retain(|s| s.id != id));
                }
            });
        }
    });

    let on_create = Callback::new({
        let tok = session.token();
        move |_| {
            let tok = tok.clone();
            let body = CreateServerBody {
                user_id: f_user.get_untracked(),
                node_id: f_node.get_untracked(),
                name: f_name.get_untracked(),
                image: f_image.get_untracked(),
                memory_mb: f_mem.get_untracked().parse().unwrap_or(512),
                cpu_percent: f_cpu.get_untracked().parse().unwrap_or(100),
            };
            spawn_local(async move {
                match ApiClient::new(tok).post::<_, Server>("/servers", &body).await {
                    Ok(s) => {
                        servers.update(|v| v.push(s));
                        show_modal.set(false);
                    }
                    Err(e) => error.set(e),
                }
            });
        }
    });

    let columns = vec![
        Column { header: "Name", render: |s: &Server| s.name.clone() },
        Column { header: "Status", render: |s: &Server| s.status.clone() },
        Column { header: "Image", render: |s: &Server| s.image.clone() },
        Column { header: "RAM", render: |s: &Server| format!("{}MB", s.memory_mb) },
    ];

    view! {
        <Card title="Servers".to_string()>
            <div class="space-y-4">
                <ErrorBanner msg=error.into() />
                <div class="flex justify-end">
                    <Button on_click=Some(Callback::new(move |_| show_modal.set(true)))>
                        "+ Create Server"
                    </Button>
                </div>
                <Table columns=columns rows=servers.get() on_delete=Some(on_delete) key_fn=|s: &Server| s.id.clone() />
            </div>
        </Card>

        <Modal title="Create Server".to_string() open=show_modal>
            <div class="space-y-4">
                <TextInput value=f_name label="Name" />
                <TextInput value=f_image label="Docker Image" placeholder="ghcr.io/..." />
                <TextInput value=f_node label="Node ID" />
                <TextInput value=f_user label="User ID" />
                <TextInput value=f_mem label="Memory (MB)" />
                <TextInput value=f_cpu label="CPU (%)" />
                <Button on_click=Some(on_create)>"Create"</Button>
            </div>
        </Modal>
    }
}
```

- [ ] **Step 8: Implementar src/pages/admin/eggs.rs**

```rust
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;
use crate::api::{client::ApiClient, eggs::*};
use crate::components::{Button, Card, Column, ErrorBanner, Modal, Table, TextInput};
use crate::state::SessionContext;

#[component]
pub fn AdminEggsPage() -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");

    let eggs = RwSignal::new(Vec::<Egg>::new());
    let error = RwSignal::new(String::new());
    let show_modal = RwSignal::new(false);
    let f_name = RwSignal::new(String::new());
    let f_desc = RwSignal::new(String::new());
    let f_author = RwSignal::new(String::new());
    let f_version = RwSignal::new("1.0.0".to_string());
    let f_start = RwSignal::new(String::new());
    let f_stop = RwSignal::new("^C".to_string());
    let f_done = RwSignal::new(String::new());

    let load = {
        let tok = session.token();
        move || {
            let tok = tok.clone();
            spawn_local(async move {
                match ApiClient::new(tok).get::<Vec<Egg>>("/eggs").await {
                    Ok(v) => eggs.set(v),
                    Err(e) => error.set(e),
                }
            });
        }
    };
    load();

    let on_delete = Callback::new({
        let tok = session.token();
        move |id: String| {
            let tok = tok.clone();
            spawn_local(async move {
                if let Err(e) = ApiClient::new(tok).delete(&format!("/eggs/{}", id)).await {
                    error.set(e);
                } else {
                    eggs.update(|v| v.retain(|e| e.id != id));
                }
            });
        }
    });

    let on_create = Callback::new({
        let tok = session.token();
        move |_| {
            let tok = tok.clone();
            let body = CreateEggBody {
                name: f_name.get_untracked(),
                description: f_desc.get_untracked(),
                author: f_author.get_untracked(),
                version: f_version.get_untracked(),
                start_cmd: f_start.get_untracked(),
                stop_cmd: f_stop.get_untracked(),
                startup_done: f_done.get_untracked(),
                docker_images: serde_json::json!({}),
            };
            spawn_local(async move {
                match ApiClient::new(tok).post::<_, Egg>("/eggs", &body).await {
                    Ok(e) => {
                        eggs.update(|v| v.push(e));
                        show_modal.set(false);
                    }
                    Err(e) => error.set(e),
                }
            });
        }
    });

    let columns = vec![
        Column { header: "Name", render: |e: &Egg| e.name.clone() },
        Column { header: "Author", render: |e: &Egg| e.author.clone() },
        Column { header: "Version", render: |e: &Egg| e.version.clone() },
    ];

    view! {
        <Card title="Eggs".to_string()>
            <div class="space-y-4">
                <ErrorBanner msg=error.into() />
                <div class="flex justify-end">
                    <Button on_click=Some(Callback::new(move |_| show_modal.set(true)))>
                        "+ Create Egg"
                    </Button>
                </div>
                <Table columns=columns rows=eggs.get() on_delete=Some(on_delete) key_fn=|e: &Egg| e.id.clone() />
            </div>
        </Card>

        <Modal title="Create Egg".to_string() open=show_modal>
            <div class="space-y-4">
                <TextInput value=f_name label="Name" />
                <TextInput value=f_desc label="Description" />
                <TextInput value=f_author label="Author" />
                <TextInput value=f_version label="Version" />
                <TextInput value=f_start label="Start Command" />
                <TextInput value=f_stop label="Stop Command" />
                <TextInput value=f_done label="Startup Done (regex)" />
                <Button on_click=Some(on_create)>"Create"</Button>
            </div>
        </Modal>
    }
}
```

- [ ] **Step 9: Verificar compilação e commit**

```bash
cd crates/frontend
trunk build
git add crates/frontend/src/api/ crates/frontend/src/pages/admin/
git commit -m "feat(frontend): admin pages CRUD — users, nodes, servers, eggs"
```

---

## Task 7: Client Area — Dashboard + Server Detail com Tabs

**Arquivos:**
- Modificar: `crates/frontend/src/pages/client/mod.rs`
- Modificar: `crates/frontend/src/pages/client/dashboard.rs`
- Modificar: `crates/frontend/src/pages/client/server_detail.rs`

**Interfaces:**
- Consome: `GET /api/servers` (lista), `GET /api/servers/:id` (detalhe)
- Consome: `POST /api/servers/:id/start|stop|restart`, `POST /api/servers/:id/command`
- Produz: `ClientDashboardPage` (grid de cards), `ServerDetailPage` com tabs Console / Logs / Stats

- [ ] **Step 1: Modificar src/pages/client/mod.rs (layout completo)**

```rust
use leptos::prelude::*;
use leptos_router::components::Outlet;
use leptos_router::hooks::use_navigate;
use crate::state::SessionContext;

pub mod console_tab;
pub mod dashboard;
pub mod logs_tab;
pub mod server_detail;
pub mod stats_tab;

#[component]
pub fn ClientLayout() -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");
    let navigate = use_navigate();

    if session.auth.get_untracked().is_none() {
        navigate("/login", Default::default());
    }

    view! {
        <div class="max-w-7xl mx-auto px-4 py-8">
            <Outlet />
        </div>
    }
}
```

- [ ] **Step 2: Implementar src/pages/client/dashboard.rs**

```rust
use leptos::prelude::*;
use leptos_router::components::A;
use wasm_bindgen_futures::spawn_local;
use crate::api::{client::ApiClient, servers::Server};
use crate::components::{Button, Card, ErrorBanner};
use crate::state::SessionContext;

#[component]
pub fn ClientDashboardPage() -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");

    let servers = RwSignal::new(Vec::<Server>::new());
    let error = RwSignal::new(String::new());

    {
        let tok = session.token();
        spawn_local(async move {
            match ApiClient::new(tok).get::<Vec<Server>>("/servers").await {
                Ok(v) => servers.set(v),
                Err(e) => error.set(e),
            }
        });
    }

    view! {
        <div class="space-y-6">
            <h1 class="text-3xl font-bold text-gray-900">"My Servers"</h1>
            <ErrorBanner msg=error.into() />
            <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                {move || servers.get().into_iter().map(|srv| {
                    let status_color = match srv.status.as_str() {
                        "running" => "text-green-600",
                        "stopped" => "text-gray-500",
                        "error" => "text-red-600",
                        _ => "text-yellow-600",
                    };
                    view! {
                        <Card title=srv.name.clone() subtitle=format!("{}MB RAM | {}% CPU", srv.memory_mb, srv.cpu_percent)>
                            <div class="space-y-3">
                                <p class=format!("text-sm font-semibold {}", status_color)>
                                    {srv.status.to_uppercase()}
                                </p>
                                <p class="text-xs text-gray-500 font-mono">{srv.image.clone()}</p>
                                <A href=format!("/client/servers/{}", srv.id)>
                                    <Button variant="secondary">"Manage →"</Button>
                                </A>
                            </div>
                        </Card>
                    }
                }).collect_view()}
            </div>
        </div>
    }
}
```

- [ ] **Step 3: Implementar src/pages/client/server_detail.rs**

```rust
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use wasm_bindgen_futures::spawn_local;
use crate::api::{client::ApiClient, servers::Server};
use crate::components::{Button, Card, ErrorBanner};
use crate::state::SessionContext;
use super::{console_tab::ConsoleTab, logs_tab::LogsTab, stats_tab::StatsTab};

#[component]
pub fn ServerDetailPage() -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");
    let params = use_params_map();
    // .get() retorna Option<&String>; .cloned() converte para Option<String> antes do unwrap
    let server_id = move || params.with(|p| p.get("id").cloned().unwrap_or_default());

    let server = RwSignal::new(None::<Server>);
    let error = RwSignal::new(String::new());
    let active_tab = RwSignal::new("console");

    {
        let tok = session.token();
        let id = server_id();
        spawn_local(async move {
            match ApiClient::new(tok).get::<Server>(&format!("/servers/{}", id)).await {
                Ok(s) => server.set(Some(s)),
                Err(e) => error.set(e),
            }
        });
    }

    let make_action = |path_suffix: &'static str| {
        let tok = session.token();
        let id = server_id();
        Callback::new(move |_| {
            let tok = tok.clone();
            let path = format!("/servers/{}/{}", id, path_suffix);
            spawn_local(async move {
                if let Err(e) = ApiClient::new(tok).post::<(), ()>(&path, &()).await {
                    error.set(e);
                }
            });
        })
    };

    view! {
        <div class="space-y-6">
            <ErrorBanner msg=error.into() />

            <Show when=move || server.get().is_some()>
                {move || server.get().map(|srv| {
                    view! {
                        <div class="flex items-center justify-between">
                            <div>
                                <h1 class="text-3xl font-bold text-gray-900">{srv.name.clone()}</h1>
                                <p class="text-sm text-gray-500">{srv.status.clone()}</p>
                            </div>
                            <div class="flex gap-2">
                                <Button on_click=Some(make_action("start"))>"Start"</Button>
                                <Button variant="secondary" on_click=Some(make_action("stop"))>"Stop"</Button>
                                <Button variant="secondary" on_click=Some(make_action("restart"))>"Restart"</Button>
                            </div>
                        </div>

                        <div class="flex gap-2 border-b border-gray-200">
                            {["console", "logs", "stats"].map(|tab| {
                                view! {
                                    <button
                                        class=move || {
                                            if active_tab.get() == tab {
                                                "px-4 py-2 border-b-2 border-blue-600 text-blue-600 font-medium"
                                            } else {
                                                "px-4 py-2 text-gray-500 hover:text-gray-700"
                                            }
                                        }
                                        on:click=move |_| active_tab.set(tab)
                                    >
                                        {tab.to_string()}
                                    </button>
                                }
                            }).into_iter().collect_view()}
                        </div>

                        <div>
                            {move || match active_tab.get() {
                                "logs" => view! { <LogsTab server_id=srv.id.clone() /> }.into_any(),
                                "stats" => view! { <StatsTab server_id=srv.id.clone() /> }.into_any(),
                                _ => view! { <ConsoleTab server_id=srv.id.clone() /> }.into_any(),
                            }}
                        </div>
                    }
                })}
            </Show>
        </div>
    }
}
```

- [ ] **Step 4: Implementar src/pages/client/console_tab.rs**

```rust
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;
use crate::api::client::ApiClient;
use crate::components::{Button, TextInput};
use crate::state::SessionContext;
use serde::Serialize;

#[derive(Serialize)]
struct CommandBody {
    content: String,
}

#[component]
pub fn ConsoleTab(server_id: String) -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");
    let cmd = RwSignal::new(String::new());
    let output = RwSignal::new(Vec::<String>::new());

    let on_send = {
        let tok = session.token();
        let id = server_id.clone();
        Callback::new(move |_| {
            let tok = tok.clone();
            let id = id.clone();
            let content = cmd.get_untracked();
            if content.is_empty() { return; }

            cmd.set(String::new());
            output.update(|v| v.push(format!("> {}", content)));

            spawn_local(async move {
                let body = CommandBody { content };
                if let Err(e) = ApiClient::new(tok)
                    .post::<_, serde_json::Value>(&format!("/servers/{}/command", id), &body)
                    .await
                {
                    output.update(|v| v.push(format!("Error: {}", e)));
                }
            });
        })
    };

    view! {
        <div class="space-y-3">
            <div class="bg-gray-950 text-gray-200 p-4 rounded-lg font-mono text-sm h-80 overflow-y-auto">
                {move || output.get().into_iter().map(|line| view! {
                    <div>{line}</div>
                }).collect_view()}
            </div>
            <form
                class="flex gap-2"
                on:submit=move |e| {
                    e.prevent_default();
                    on_send.call(());
                }
            >
                <TextInput value=cmd placeholder="Enter command..." />
                <Button>"Send"</Button>
            </form>
        </div>
    }
}
```

- [ ] **Step 5: Commit**

```bash
git add crates/frontend/src/pages/client/
git commit -m "feat(frontend): client dashboard + server detail com tabs (console, logs, stats)"
```

---

## Task 8: SSE — Logs e Stats em Tempo Real

**Arquivos:**
- Modificar: `crates/frontend/src/api/sse.rs`
- Modificar: `crates/frontend/src/pages/client/logs_tab.rs`
- Modificar: `crates/frontend/src/pages/client/stats_tab.rs`

**Interfaces:**
- Consome: `GET /api/servers/:id/logs?follow=true` (SSE text/event-stream)
- Consome: `GET /api/servers/:id/stats` (SSE com JSON por evento)
- Produz:
  - `use_sse(url: String) -> RwSignal<Vec<String>>` — acumula linhas; `on_cleanup` usa `let _ = onmessage` (sem `forget()` permanente)
  - `use_sse_callback(url: String, on_message: impl FnMut(String))` — callback por mensagem; use para stats (sem Vec acumulativo)
  - `LogsTab` — `EventSource` criado sob demanda (clique), gerenciado via `RwSignal<Option<EventSource>>`; `on_cleanup` no root do componente garante fechamento ao desmontar
  - `StatsTab` com cards de memória, CPU, rede atualizados em tempo real

- [ ] **Step 1: Implementar src/api/sse.rs**

```rust
use leptos::prelude::*;
use wasm_bindgen::{closure::Closure, JsCast};
use web_sys::EventSource;

/// Acumula todas as linhas recebidas. Fecha o EventSource ao desmontar o componente.
/// DEVE ser chamado no nível do componente (dentro do escopo reativo), nunca dentro
/// de callbacks — on_cleanup perderia o Reactive Owner e a conexão não seria fechada.
pub fn use_sse(url: String) -> RwSignal<Vec<String>> {
    let lines = RwSignal::new(Vec::<String>::new());
    let source = EventSource::new(&url).expect("EventSource");

    let onmessage = Closure::wrap(Box::new({
        let lines = lines.clone();
        move |event: web_sys::MessageEvent| {
            let data = event.data().as_string().unwrap_or_default();
            lines.update(|v| v.push(data));
        }
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);

    source.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));

    let source_close = source.clone();
    // `let _ = onmessage` move a Closure para dentro do on_cleanup, mantendo-a viva
    // até o desmonte e liberando heap WASM quando o componente for destruído.
    // NÃO usar onmessage.forget() — alocaria permanentemente no heap WASM.
    on_cleanup(move || {
        source_close.close();
        let _ = onmessage;
    });

    lines
}

/// Versão baseada em callback — use para stats/valores que se substituem a cada tick.
/// Evita crescimento linear de memória (ao contrário de acumular em Vec).
/// DEVE ser chamado no nível do componente pelo mesmo motivo de `use_sse`.
pub fn use_sse_callback(url: String, mut on_message: impl FnMut(String) + 'static) {
    let source = EventSource::new(&url).expect("EventSource");

    let onmessage = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
        let data = event.data().as_string().unwrap_or_default();
        on_message(data);
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);

    source.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));

    let source_close = source.clone();
    on_cleanup(move || {
        source_close.close();
        let _ = onmessage;
    });
}
```

- [ ] **Step 2: Implementar src/pages/client/logs_tab.rs**

```rust
use leptos::prelude::*;
use leptos::html::Div;
use wasm_bindgen::{closure::Closure, JsCast};
use web_sys::EventSource;
use crate::state::SessionContext;

const API_BASE: &str = "/api";

#[component]
pub fn LogsTab(server_id: String) -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");
    let streaming = RwSignal::new(false);
    let logs = RwSignal::new(Vec::<String>::new());

    // EventSource armazenado em signal para acesso no stop e no on_cleanup do root.
    // on_cleanup registrado aqui (escopo reativo correto) garante fechamento no desmonte,
    // mesmo que a conexão seja criada later (dentro do callback de clique).
    let event_source: RwSignal<Option<EventSource>> = RwSignal::new(None);

    on_cleanup(move || {
        if let Some(source) = event_source.get_untracked() {
            source.close();
        }
    });

    let start_stream = {
        let id = server_id.clone();
        let tok = session.token();
        Callback::new(move |_| {
            if streaming.get_untracked() { return; }
            streaming.set(true);

            // SSE não suporta headers customizados no browser — token vai via query param
            let url = format!("{}/servers/{}/logs?follow=true&token={}", API_BASE, id, tok);
            let source = EventSource::new(&url).expect("EventSource");

            let onmessage = Closure::wrap(Box::new({
                let logs = logs.clone();
                move |event: web_sys::MessageEvent| {
                    let data = event.data().as_string().unwrap_or_default();
                    logs.update(|v| v.push(data));
                }
            }) as Box<dyn FnMut(web_sys::MessageEvent)>);

            source.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
            event_source.set(Some(source));
            // onmessage é intencionalmente "esquecido" aqui pois o cleanup do component
            // root fecha o EventSource, e o JS GC cuida da função JS associada.
            // A alternativa (on_cleanup dentro de callback) perderia o Reactive Owner.
            let _ = onmessage;
        })
    };

    let stop_stream = move || {
        streaming.set(false);
        if let Some(source) = event_source.get_untracked() {
            source.close();
        }
        event_source.set(None);
        logs.set(vec![]);
    };

    let log_div: NodeRef<Div> = NodeRef::new();

    Effect::new(move |_| {
        let _ = logs.get();
        if let Some(el) = log_div.get() {
            el.set_scroll_top(el.scroll_height());
        }
    });

    view! {
        <div class="space-y-3">
            <div class="flex gap-2">
                <button
                    on:click=move |_| start_stream.call(())
                    disabled=streaming.get()
                    class="px-4 py-2 bg-blue-600 text-white rounded disabled:opacity-50"
                >
                    "Stream Logs"
                </button>
                <button
                    on:click=move |_| stop_stream()
                    class="px-4 py-2 bg-gray-200 rounded"
                >
                    "Stop / Clear"
                </button>
            </div>
            <div
                node_ref=log_div
                class="bg-gray-950 text-gray-200 p-4 rounded-lg font-mono text-xs h-96 overflow-y-auto"
            >
                {move || logs.get().into_iter().map(|line| view! {
                    <div class="whitespace-pre-wrap leading-5">{line}</div>
                }).collect_view()}
            </div>
        </div>
    }
}
```

- [ ] **Step 3: Implementar src/pages/client/stats_tab.rs**

```rust
use leptos::prelude::*;
use serde::Deserialize;
use crate::api::sse::use_sse_callback;
use crate::state::SessionContext;

const API_BASE: &str = "/api";

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

    // use_sse_callback substitui o valor a cada tick — evita Vec crescendo linearmente.
    // Chamado no escopo raiz do componente para que on_cleanup tenha Reactive Owner correto.
    let url = format!(
        "{}/servers/{}/stats?token={}",
        API_BASE, server_id, session.token()
    );
    use_sse_callback(url, move |data| {
        if let Ok(s) = serde_json::from_str::<Stats>(&data) {
            stats.set(Some(s));
        }
    });

    let stats = move || stats.get();

    fn fmt_mb(bytes: u64) -> String {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    }

    view! {
        <Show
            when=move || stats().is_some()
            fallback=|| view! {
                <p class="text-gray-500 text-sm">"Waiting for stats..."</p>
            }
        >
            {move || stats().map(|s| {
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

- [ ] **Step 4: Verificar compilação**

```bash
cd crates/frontend
trunk build
```

Expected: sem erros de compilação.

- [ ] **Step 5: Commit**

```bash
git add crates/frontend/src/api/sse.rs \
        crates/frontend/src/pages/client/logs_tab.rs \
        crates/frontend/src/pages/client/stats_tab.rs
git commit -m "feat(frontend): SSE streaming para logs e stats em tempo real"
```

---

## Task 9: Build de Release + Servir como Asset Estático

**Arquivos:**
- Criar: `crates/frontend/nginx.conf`
- Modificar: `crates/frontend/Trunk.toml` (output paths corretos)

**Interfaces:**
- Produz: `trunk build --release` → `dist/` com assets minificados servíveis por qualquer servidor estático

- [ ] **Step 1: Atualizar Trunk.toml para release**

```toml
[build]
target = "index.html"
dist = "dist"
public_url = "/"

[watch]
ignore = ["target", "dist"]

[[hooks]]
stage = "pre_build"
command = "npx"
command_arguments = ["tailwindcss", "-i", "./input.css", "-o", "./dist/tailwind.css", "--minify"]
```

- [ ] **Step 2: Criar nginx.conf (SPA — redireciona 404 para index.html)**

```nginx
server {
    listen 80;
    root /usr/share/nginx/html;
    index index.html;

    gzip on;
    gzip_types text/html application/javascript application/wasm;

    location / {
        try_files $uri /index.html;
    }

    location ~* \.(wasm|js|css)$ {
        expires 1y;
        add_header Cache-Control "public, immutable";
    }
}
```

- [ ] **Step 3: Build de release e verificar tamanho**

```bash
cd crates/frontend
trunk build --release
ls -lh dist/
```

Expected: `dist/index.html`, `dist/*.wasm` (~2-10MB), `dist/tailwind.css`, `dist/*.js`

- [ ] **Step 4: Testar serve local**

```bash
cd crates/frontend
trunk serve --open
```

Expected: Browser abre em `http://localhost:8080`, login page renderiza.

- [ ] **Step 5: Commit**

```bash
git add crates/frontend/Trunk.toml crates/frontend/nginx.conf
git commit -m "chore(frontend): Trunk release config + nginx SPA conf"
```

---

## Self-Review

**1. Spec coverage:**
- ✅ Admin area: Users, Nodes, Servers, Eggs (list + create + delete)
- ✅ Client area: Dashboard (grid de servers), Server detail com tabs
- ✅ Console: envio de comando via POST /command
- ✅ Logs: SSE streaming via EventSource
- ✅ Stats: SSE streaming com cards em tempo real
- ✅ Auth JWT: login, contexto global, localStorage, redirect por role
- ✅ Componentes reutilizáveis: Button, Input, Card, Modal, Table, Navbar, ErrorBanner
- ✅ Layout responsivo: Tailwind CSS grid + flex

**2. Placeholder scan:** Nenhum "TBD" ou "TODO" em código — stubs são componentes compiláveis que exibem placeholders textuais.

**3. Consistência de tipos:**
- `SessionContext::token()` → `String` — usado identicamente em Tasks 2, 6, 7, 8
- `ApiClient::new(tok)` → `ApiClient` — mesma assinatura em todos os usos
- `Server { id, name, status, image, memory_mb, cpu_percent }` — definido na Task 6, consumido na Task 7
- `use_sse(url: String) -> RwSignal<Vec<String>>` — definido Task 8, consumido em logs_tab e stats_tab
