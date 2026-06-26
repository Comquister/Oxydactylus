# Frontend SPA Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Criar um frontend de página única (SPA) completo, moderno e altamente responsivo utilizando HTML5, CSS3 vanila e JavaScript, servido diretamente pelo backend Axum do Oxydactylus.

**Architecture:** O backend Axum servirá os arquivos estáticos localizados em `crates/panel/static/` na rota `/` com suporte a fallback de SPA. O frontend se comunicará via REST APIs JSON e Server-Sent Events (SSE) para logs em tempo real. O estado do token JWT de autenticação será armazenado no `localStorage`.

**Tech Stack:** Rust 2021, Axum 0.7, tower-http 0.5 (recurso `fs`), HTML5, CSS3, ES6 JavaScript.

## Global Constraints

- O frontend deve ser servido pelo Axum em `/` (com suporte a fallback para permitir roteamento por hash).
- Toda a estilização deve ser feita com Vanilla CSS estruturado (paleta baseada em HSL, tema escuro premium com glassmorphism).
- Sem dependências de frameworks JS (React, Vue) ou TailwindCSS; manter leve, limpo e direto.
- Comunicação de logs em tempo real via `EventSource` (SSE).

---

### Task 1: Servir Arquivos Estáticos no Axum

**Files:**
- Modify: `crates/panel/Cargo.toml`
- Modify: `crates/panel/src/lib.rs`
- Create: `crates/panel/static/index.html`

**Interfaces:**
- Consumes: `AppState` de `lib.rs`
- Produces: Rota `/` no Axum servindo `crates/panel/static/index.html` e arquivos estáticos (fallback para SPA)

- [ ] **Step 1: Adicionar dependência `tower-http`**

No `crates/panel/Cargo.toml`, sob `[dependencies]`, adicione:
```toml
tower-http = { version = "0.5", features = ["fs"] }
```

- [ ] **Step 2: Criar arquivo inicial HTML**

Crie `crates/panel/static/index.html`:
```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Oxydactylus Panel</title>
</head>
<body>
    <div id="app">
        <h1>Oxydactylus Loading...</h1>
    </div>
</body>
</html>
```

- [ ] **Step 3: Configurar Axum para servir estáticos em `lib.rs`**

Modifique `crates/panel/src/lib.rs` para incluir o serviço de arquivos estáticos. Encontre os `use` existentes e adicione:
```rust
use tower_http::services::{ServeDir, ServeFile};
```

Modifique a função `router` para:
```rust
pub fn router(state: AppState) -> axum::Router {
    axum::Router::new()
        .nest("/auth", auth::auth_router())
        .nest("/api/users", users::users_router())
        .nest("/api/nodes", nodes::nodes_router())
        .nest("/api/servers", servers::servers_router())
        .nest("/api/eggs", eggs::eggs_router())
        .route("/api/me", get(users::me))
        .nest_service(
            "/",
            ServeDir::new("crates/panel/static")
                .not_found_service(ServeFile::new("crates/panel/static/index.html")),
        )
        .with_state(state)
}
```

- [ ] **Step 4: Escrever teste de rota estática**

Adicione no bloco de testes de `crates/panel/src/lib.rs` (ou crie um se não houver):
```rust
#[sqlx::test(migrations = "./migrations")]
async fn test_serves_static_index(pool: sqlx::PgPool) {
    let state = AppState {
        db: pool,
        jwt_secret: "test-secret-at-least-32-chars-long!!".to_string(),
    };
    let app = router(state);
    let req = axum::http::Request::builder()
        .uri("/")
        .body(axum::body::Body::empty())
        .unwrap();
    use tower::ServiceExt;
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), axum::http::StatusCode::OK);
}
```

- [ ] **Step 5: Executar testes e verificar compilação**

Execute:
```bash
cargo test -p oxy-panel --lib test_serves_static_index 2>&1
```
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/panel/Cargo.toml crates/panel/src/lib.rs crates/panel/static/index.html
git commit -m "feat(panel): serve static directory with tower-http and SPA fallback"
```

---

### Task 2: Design System e SPA Shell (index.html + style.css)

**Files:**
- Create: `crates/panel/static/style.css`
- Modify: `crates/panel/static/index.html`

**Interfaces:**
- Consumes: Arquivos estáticos em `/` (Task 1)
- Produces: CSS layout global baseado em HSL, tema escuro e containers invisíveis das visões `#login-view`, `#dashboard-view`, `#server-view`

- [ ] **Step 1: Criar o Design System CSS**

Crie `crates/panel/static/style.css`:
```css
@import url('https://fonts.googleapis.com/css2?family=Outfit:wght@300;400;600;800&family=JetBrains+Mono:wght@400;700&display=swap');

:root {
    --bg-primary: hsl(222, 47%, 11%);
    --bg-secondary: hsl(223, 47%, 16%);
    --bg-glass: hsla(223, 47%, 16%, 0.65);
    --border-color: hsl(223, 30%, 25%);
    --text-primary: hsl(210, 40%, 98%);
    --text-secondary: hsl(215, 20%, 65%);
    --accent: hsl(250, 89%, 65%);
    --accent-glow: hsla(250, 89%, 65%, 0.15);
    --success: hsl(142, 76%, 45%);
    --error: hsl(346, 84%, 61%);
    --warning: hsl(45, 93%, 47%);
    --font-sans: 'Outfit', sans-serif;
    --font-mono: 'JetBrains Mono', monospace;
}

* {
    box-sizing: border-box;
    margin: 0;
    padding: 0;
}

body {
    background-color: var(--bg-primary);
    color: var(--text-primary);
    font-family: var(--font-sans);
    line-height: 1.6;
    overflow-x: hidden;
    background-image: 
        radial-gradient(at 0% 0%, hsla(253,16%,7%,1) 0, transparent 50%), 
        radial-gradient(at 50% 0%, hsla(255,89%,65%,0.07) 0, transparent 50%);
}

.glass-panel {
    background: var(--bg-glass);
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    border: 1px solid var(--border-color);
    border-radius: 12px;
}

.view-container {
    display: none;
    max-width: 1200px;
    margin: 40px auto;
    padding: 20px;
    animation: fadeIn 0.4s ease;
}

@keyframes fadeIn {
    from { opacity: 0; transform: translateY(10px); }
    to { opacity: 1; transform: translateY(0); }
}

/* Formulários */
input, button, select, textarea {
    font-family: inherit;
}

input[type="text"], input[type="email"], input[type="password"], select, textarea {
    width: 100%;
    padding: 12px 16px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: 8px;
    color: var(--text-primary);
    outline: none;
    transition: all 0.3s ease;
}

input:focus, select:focus, textarea:focus {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-glow);
}

button {
    cursor: pointer;
    padding: 12px 24px;
    border: none;
    border-radius: 8px;
    font-weight: 600;
    transition: all 0.3s ease;
}

.btn-primary {
    background: var(--accent);
    color: white;
}

.btn-primary:hover {
    filter: brightness(1.1);
    transform: translateY(-1px);
}

.btn-secondary {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    color: var(--text-primary);
}

.btn-secondary:hover {
    background: var(--border-color);
}
```

- [ ] **Step 2: Modificar `index.html` com o HTML Shell**

Substitua `crates/panel/static/index.html` por:
```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Oxydactylus Panel</title>
    <link rel="stylesheet" href="/style.css">
</head>
<body>
    <header class="glass-panel" style="margin: 20px; padding: 15px 30px; display: flex; justify-content: space-between; align-items: center;">
        <h2 style="font-weight: 800; tracking-spacing: 1px; color: var(--accent);">OXYDACTYLUS</h2>
        <nav id="main-nav" style="display: none; gap: 20px;">
            <a href="#" style="color: var(--text-primary); text-decoration: none; font-weight: 600;">Dashboard</a>
            <button id="logout-btn" class="btn-secondary" style="padding: 6px 12px;">Logout</button>
        </nav>
    </header>

    <!-- Login View -->
    <main id="login-view" class="view-container glass-panel" style="max-width: 450px; margin-top: 100px; padding: 40px;">
        <h2 style="margin-bottom: 24px; text-align: center;">Acessar Painel</h2>
        <form id="login-form" style="display: flex; flex-direction: column; gap: 20px;">
            <div>
                <label style="display: block; margin-bottom: 8px; font-size: 0.9rem; color: var(--text-secondary);">Email</label>
                <input type="email" id="login-email" required placeholder="admin@example.com">
            </div>
            <div>
                <label style="display: block; margin-bottom: 8px; font-size: 0.9rem; color: var(--text-secondary);">Senha</label>
                <input type="password" id="login-password" required placeholder="••••••••">
            </div>
            <button type="submit" class="btn-primary">Entrar</button>
            <p id="login-error" style="color: var(--error); text-align: center; display: none;"></p>
        </form>
    </main>

    <!-- Dashboard View -->
    <main id="dashboard-view" class="view-container">
        <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 30px;">
            <h1>Seus Servidores</h1>
            <div id="admin-actions" style="display: none;">
                <button id="show-create-server-btn" class="btn-primary">+ Criar Servidor</button>
                <button id="nav-nodes-btn" class="btn-secondary">Gerenciar Nodes</button>
                <button id="nav-users-btn" class="btn-secondary">Gerenciar Usuários</button>
            </div>
        </div>
        <div id="servers-list" style="display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 20px;">
            <!-- Servidores populados dinamicamente -->
        </div>
    </main>

    <!-- Server Detail View -->
    <main id="server-view" class="view-container">
        <!-- Estrutura básica do painel do servidor -->
        <h1 id="server-title">Nome do Servidor</h1>
        <p id="server-status" style="margin-bottom: 20px;">Status: ...</p>
    </main>

    <script src="/app.js"></script>
</body>
</html>
```

- [ ] **Step 3: Criar um arquivo JavaScript temporário para não quebrar a importação**

Crie `crates/panel/static/app.js`:
```javascript
console.log("Oxydactylus core logic loading...");
```

- [ ] **Step 4: Compilar e rodar visualização local**

Execute:
```bash
cargo run --bin oxydactylus -- -c config.toml
```
Abra `http://localhost:3000` no browser e confirme que a página carrega o esqueleto inicial com título e formulário de Login de forma harmoniosa com o CSS.

- [ ] **Step 5: Commit**

```bash
git add crates/panel/static/style.css crates/panel/static/index.html crates/panel/static/app.js
git commit -m "feat(frontend): design system and HTML structural views shell"
```

---

### Task 3: Autenticação, Roteamento e Requisições (`app.js`)

**Files:**
- Modify: `crates/panel/static/app.js`

**Interfaces:**
- Consumes: Endpoints do backend Axum (`POST /auth/login`, `GET /api/me`)
- Produces: Roteamento baseado em hash (ex: `#login`, `#dashboard`), tokens armazenados no `localStorage` e wrapper `apiFetch` que inclui o token Bearer automaticamente

- [ ] **Step 1: Implementar o Roteamento e Inicialização**

Substitua `crates/panel/static/app.js` por:
```javascript
const API_URL = "";

// State
let state = {
    token: localStorage.getItem("oxy_token") || null,
    user: null
};

// Router
function route() {
    const hash = window.location.hash || "#dashboard";
    
    // Hide all views
    document.querySelectorAll(".view-container").forEach(el => el.style.display = "none");
    document.getElementById("main-nav").style.display = state.token ? "flex" : "none";

    if (!state.token) {
        window.location.hash = "#login";
        document.getElementById("login-view").style.display = "block";
        return;
    }

    if (hash === "#login" && state.token) {
        window.location.hash = "#dashboard";
        return;
    }

    if (hash === "#dashboard") {
        document.getElementById("dashboard-view").style.display = "block";
        loadDashboard();
    } else if (hash.startsWith("#server/")) {
        const serverId = hash.split("/")[1];
        document.getElementById("server-view").style.display = "block";
        loadServerView(serverId);
    }
}

window.addEventListener("hashchange", route);
window.addEventListener("load", async () => {
    if (state.token) {
        try {
            await fetchCurrentUser();
        } catch (e) {
            logout();
        }
    }
    route();
});
```

- [ ] **Step 2: Adicionar o Helper de Requisições Seguras `apiFetch`**

Adicione em `crates/panel/static/app.js`:
```javascript
async function apiFetch(path, options = {}) {
    const headers = new Headers(options.headers || {});
    if (state.token) {
        headers.append("Authorization", `Bearer ${state.token}`);
    }
    if (options.body && !(options.body instanceof FormData)) {
        headers.append("Content-Type", "application/json");
    }

    const res = await fetch(`${API_URL}${path}`, {
        ...options,
        headers
    });

    if (res.status === 401) {
        logout();
        throw new Error("Sessão expirada");
    }

    if (!res.ok) {
        const err = await res.json().catch(() => ({ message: "Erro desconhecido" }));
        throw new Error(err.message || res.statusText);
    }

    if (res.status === 204) return null;
    return res.json();
}

async function fetchCurrentUser() {
    const user = await apiFetch("/api/me");
    state.user = user;
    if (user.is_admin) {
        document.getElementById("admin-actions").style.display = "flex";
    } else {
        document.getElementById("admin-actions").style.display = "none";
    }
}

function logout() {
    state.token = null;
    state.user = null;
    localStorage.removeItem("oxy_token");
    window.location.hash = "#login";
    route();
}

document.getElementById("logout-btn").addEventListener("click", logout);
```

- [ ] **Step 3: Implementar Login**

Adicione em `crates/panel/static/app.js`:
```javascript
document.getElementById("login-form").addEventListener("submit", async (e) => {
    e.preventDefault();
    const email = document.getElementById("login-email").value;
    const password = document.getElementById("login-password").value;
    const errorEl = document.getElementById("login-error");
    errorEl.style.display = "none";

    try {
        const res = await apiFetch("/auth/login", {
            method: "POST",
            body: JSON.stringify({ email, password })
        });
        state.token = res.access_token;
        localStorage.setItem("oxy_token", res.access_token);
        await fetchCurrentUser();
        window.location.hash = "#dashboard";
        route();
    } catch (err) {
        errorEl.textContent = err.message;
        errorEl.style.display = "block";
    }
});
```

- [ ] **Step 4: Adicionar stubs das views auxiliares**

Adicione ao final de `crates/panel/static/app.js`:
```javascript
function loadDashboard() {
    console.log("Loading dashboard...");
}

function loadServerView(id) {
    console.log("Loading server: " + id);
}
```

- [ ] **Step 5: Testar login via curl/browser**

Selecione um usuário administrador no banco. Faça login no formulário no navegador em `http://localhost:3000`.
Expected: Login funciona, token é armazenado no localStorage, a hash redireciona para `#dashboard` e o console imprime "Loading dashboard...".

- [ ] **Step 6: Commit**

```bash
git add crates/panel/static/app.js
git commit -m "feat(frontend): authentication middleware, client-side routing, and JWT token storage"
```

---

### Task 4: Lista de Servidores e Painel Administrativo

**Files:**
- Modify: `crates/panel/static/index.html`
- Modify: `crates/panel/static/app.js`

**Interfaces:**
- Consumes: `GET /api/servers`, `GET /api/nodes`, `GET /api/users`, `POST /api/servers`
- Produces: Tabela dinâmica de servidores no dashboard e modais administrativos para nodes/users

- [ ] **Step 1: Adicionar modal de criação de servidor no HTML**

Adicione dentro da div `#dashboard-view` de `crates/panel/static/index.html` (abaixo da lista de servidores):
```html
    <!-- Modal Criar Servidor -->
    <div id="create-server-modal" class="glass-panel" style="display: none; position: fixed; top: 50%; left: 50%; transform: translate(-50%, -50%); width: 90%; max-width: 500px; padding: 30px; z-index: 1000;">
        <h2 style="margin-bottom: 20px;">Provisionar Servidor</h2>
        <form id="create-server-form" style="display: flex; flex-direction: column; gap: 15px;">
            <div>
                <label>Nome</label>
                <input type="text" id="srv-name" required placeholder="ex: survival-1">
            </div>
            <div>
                <label>Node ID</label>
                <select id="srv-node" required></select>
            </div>
            <div>
                <label>Imagem Docker</label>
                <input type="text" id="srv-image" required value="itzg/minecraft-server:latest">
            </div>
            <div>
                <label>Memória (MB)</label>
                <input type="number" id="srv-memory" required value="1024">
            </div>
            <div>
                <label>Limite CPU (%)</label>
                <input type="number" id="srv-cpu" required value="100">
            </div>
            <div style="display: flex; gap: 10px; justify-content: flex-end;">
                <button type="button" id="close-modal-btn" class="btn-secondary">Cancelar</button>
                <button type="submit" class="btn-primary">Provisionar</button>
            </div>
        </form>
    </div>
```

- [ ] **Step 2: Carregar Lista de Servidores**

Substitua a função `loadDashboard` em `crates/panel/static/app.js`:
```javascript
async function loadDashboard() {
    const listEl = document.getElementById("servers-list");
    listEl.innerHTML = "<p>Carregando servidores...</p>";

    try {
        const servers = await apiFetch("/api/servers");
        listEl.innerHTML = "";
        if (servers.length === 0) {
            listEl.innerHTML = "<p>Nenhum servidor encontrado.</p>";
            return;
        }

        servers.forEach(srv => {
            const card = document.createElement("div");
            card.className = "glass-panel";
            card.style.padding = "20px";
            card.style.display = "flex";
            card.style.flexDirection = "column";
            card.style.justifyContent = "space-between";
            card.innerHTML = `
                <div>
                    <h3 style="margin-bottom: 5px;">${srv.name}</h3>
                    <p style="font-size: 0.85rem; color: var(--text-secondary); margin-bottom: 10px;">ID: ${srv.id}</p>
                    <span style="padding: 4px 8px; border-radius: 4px; font-size: 0.8rem; font-weight: 600;" class="status-${srv.status}">${srv.status.toUpperCase()}</span>
                </div>
                <a href="#server/${srv.id}" class="btn-primary" style="text-decoration: none; text-align: center; margin-top: 20px; font-size: 0.9rem; padding: 8px;">Gerenciar</a>
            `;
            listEl.appendChild(card);
        });
    } catch (err) {
        listEl.innerHTML = `<p style="color: var(--error)">Erro: ${err.message}</p>`;
    }
}
```

- [ ] **Step 3: Lógica do Modal de Criação de Servidor (Admin)**

Adicione no arquivo `crates/panel/static/app.js`:
```javascript
const modal = document.getElementById("create-server-modal");
document.getElementById("show-create-server-btn").addEventListener("click", async () => {
    const select = document.getElementById("srv-node");
    select.innerHTML = "<option>Carregando nodes...</option>";
    modal.style.display = "block";
    try {
        const nodes = await apiFetch("/api/nodes");
        select.innerHTML = nodes.map(n => `<option value="${n.id}">${n.name} (${n.grpc_addr})</option>`).join("");
    } catch (err) {
        select.innerHTML = `<option>Erro ao carregar: ${err.message}</option>`;
    }
});

document.getElementById("close-modal-btn").addEventListener("click", () => {
    modal.style.display = "none";
});

document.getElementById("create-server-form").addEventListener("submit", async (e) => {
    e.preventDefault();
    const payload = {
        name: document.getElementById("srv-name").value,
        node_id: document.getElementById("srv-node").value,
        image: document.getElementById("srv-image").value,
        memory_mb: parseInt(document.getElementById("srv-memory").value),
        cpu_percent: parseInt(document.getElementById("srv-cpu").value),
    };

    try {
        await apiFetch("/api/servers", {
            method: "POST",
            body: JSON.stringify(payload)
        });
        modal.style.display = "none";
        loadDashboard();
    } catch (err) {
        alert("Erro ao criar: " + err.message);
    }
});
```

- [ ] **Step 4: Testar visualmente**
Faça login com sua conta de teste administrador, abra o modal de criação de servidor e crie um servidor.
Expected: O servidor é cadastrado no banco, o modal fecha e o card aparece na lista principal.

- [ ] **Step 5: Commit**

```bash
git add crates/panel/static/index.html crates/panel/static/app.js
git commit -m "feat(frontend): dynamic dashboard server lists and provision modal for admins"
```

---

### Task 5: Visão do Servidor (Logs SSE, Terminal e Controle)

**Files:**
- Modify: `crates/panel/static/index.html`
- Modify: `crates/panel/static/app.js`

**Interfaces:**
- Consumes: SSE `GET /api/servers/:id/logs`, `GET /api/servers/:id/stats`, `POST /api/servers/:id/start`, `POST /api/servers/:id/stop`, `POST /api/servers/:id/restart`
- Produces: Console de logs estilo terminal, monitor de recursos em tempo real e controle do lifecycle do servidor (Start/Stop/Restart)

- [ ] **Step 1: Adicionar estrutura do terminal e painel de recursos**

Substitua o container `#server-view` em `crates/panel/static/index.html` por:
```html
    <!-- Server Detail View -->
    <main id="server-view" class="view-container" style="max-width: 1100px;">
        <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 20px;">
            <div>
                <h1 id="server-name" style="font-weight: 800; color: var(--accent);">Carregando...</h1>
                <p id="server-id-label" style="font-size: 0.85rem; color: var(--text-secondary);"></p>
            </div>
            <div style="display: flex; gap: 10px;">
                <button id="btn-start" class="btn-primary" style="background: var(--success);">Iniciar</button>
                <button id="btn-stop" class="btn-primary" style="background: var(--error);">Parar</button>
                <button id="btn-restart" class="btn-secondary">Reiniciar</button>
            </div>
        </div>

        <div style="display: grid; grid-template-columns: 3fr 1fr; gap: 20px;">
            <!-- Console / Logs -->
            <div class="glass-panel" style="padding: 20px; display: flex; flex-direction: column; height: 500px;">
                <h3 style="margin-bottom: 10px;">Console do Servidor</h3>
                <div id="console-output" style="background: hsl(222, 47%, 5%); flex: 1; border-radius: 6px; padding: 15px; font-family: var(--font-mono); font-size: 0.85rem; overflow-y: auto; color: #a6accd; display: flex; flex-direction: column; gap: 4px;"></div>
                <div style="display: flex; margin-top: 10px; gap: 10px;">
                    <input type="text" id="console-input" placeholder="Enviar comando..." style="flex: 1; font-family: var(--font-mono);">
                    <button id="btn-send-cmd" class="btn-primary">Enviar</button>
                </div>
            </div>

            <!-- Stats / Recursos -->
            <div style="display: flex; flex-direction: column; gap: 20px;">
                <div class="glass-panel" style="padding: 20px;">
                    <h3 style="margin-bottom: 15px;">Uso de Recursos</h3>
                    <div style="margin-bottom: 15px;">
                        <div style="display: flex; justify-content: space-between; margin-bottom: 5px;">
                            <span>CPU</span>
                            <span id="cpu-label">0%</span>
                        </div>
                        <div style="background: var(--bg-secondary); border-radius: 4px; height: 8px; overflow: hidden;">
                            <div id="cpu-bar" style="width: 0%; background: var(--accent); height: 100%; transition: width 0.5s ease;"></div>
                        </div>
                    </div>
                    <div>
                        <div style="display: flex; justify-content: space-between; margin-bottom: 5px;">
                            <span>Memória</span>
                            <span id="mem-label">0 MB</span>
                        </div>
                        <div style="background: var(--bg-secondary); border-radius: 4px; height: 8px; overflow: hidden;">
                            <div id="mem-bar" style="width: 0%; background: var(--accent); height: 100%; transition: width 0.5s ease;"></div>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    </main>
```

- [ ] **Step 2: Implementar Log Streaming SSE e Stats Polling**

Substitua a função `loadServerView` em `crates/panel/static/app.js` e adicione a lógica de streaming:
```javascript
let logSource = null;
let statsInterval = null;

async function loadServerView(id) {
    // Limpar conexões anteriores
    if (logSource) logSource.close();
    if (statsInterval) clearInterval(statsInterval);

    const consoleOut = document.getElementById("console-output");
    consoleOut.innerHTML = "<p style='color: var(--text-secondary)'>Conectando ao console do servidor...</p>";

    try {
        const srv = await apiFetch(`/api/servers/${id}`);
        document.getElementById("server-name").textContent = srv.name;
        document.getElementById("server-id-label").textContent = `ID: ${srv.id}`;

        // Iniciar SSE
        logSource = new EventSource(`${API_URL}/api/servers/${id}/logs?follow=true`);
        
        logSource.onmessage = (e) => {
            const line = document.createElement("div");
            line.textContent = e.data;
            consoleOut.appendChild(line);
            consoleOut.scrollTop = consoleOut.scrollHeight;
        };

        logSource.onerror = () => {
            const line = document.createElement("div");
            line.style.color = "var(--error)";
            line.textContent = "[SSE: Conexão perdida]";
            consoleOut.appendChild(line);
        };

        // Iniciar poll de Recursos / Stats
        updateStats(id);
        statsInterval = setInterval(() => updateStats(id), 2000);

        // Bind do lifecycle
        document.getElementById("btn-start").onclick = () => controlLifecycle(id, "start");
        document.getElementById("btn-stop").onclick = () => controlLifecycle(id, "stop");
        document.getElementById("btn-restart").onclick = () => controlLifecycle(id, "restart");
        
        // Command sender
        document.getElementById("btn-send-cmd").onclick = () => sendCommand(id);
        document.getElementById("console-input").onkeypress = (e) => {
            if (e.key === "Enter") sendCommand(id);
        };

    } catch (err) {
        consoleOut.innerHTML = `<p style="color: var(--error)">Erro ao carregar servidor: ${err.message}</p>`;
    }
}

async function updateStats(id) {
    try {
        const stats = await apiFetch(`/api/servers/${id}/stats`);
        const cpuPct = Math.round(stats.cpu_percent || 0);
        const memMb = Math.round((stats.memory_bytes || 0) / 1024 / 1024);

        document.getElementById("cpu-label").textContent = `${cpuPct}%`;
        document.getElementById("cpu-bar").style.width = `${Math.min(cpuPct, 100)}%`;

        document.getElementById("mem-label").textContent = `${memMb} MB`;
        document.getElementById("mem-bar").style.width = `80%`; // escala fictícia
    } catch (e) {
        console.warn("Falha ao recuperar estatísticas:", e);
    }
}

async function controlLifecycle(id, action) {
    try {
        await apiFetch(`/api/servers/${id}/${action}`, { method: "POST" });
    } catch (err) {
        alert(`Falha ao executar ${action}: ` + err.message);
    }
}

async function sendCommand(id) {
    const input = document.getElementById("console-input");
    const cmd = input.value;
    if (!cmd) return;
    
    try {
        await apiFetch(`/api/servers/${id}/command`, {
            method: "POST",
            body: JSON.stringify({ content: cmd })
        });
        input.value = "";
    } catch (err) {
        alert("Erro ao enviar comando: " + err.message);
    }
}
```

- [ ] **Step 3: Testar integração do console**
Inicie o console do servidor de testes, envie comandos e clique nos botões Iniciar/Parar.
Expected: Chamadas gRPC ao node acontecem em background, as barras de CPU e Memória mostram atividade e os logs aparecem sob demanda no terminal.

- [ ] **Step 4: Commit**

```bash
git add crates/panel/static/index.html crates/panel/static/app.js
git commit -m "feat(frontend): server detail panel with SSE logs and lifecycle control buttons"
```

---

### Task 6: Gerenciamento de Subusuários

**Files:**
- Modify: `crates/panel/static/index.html`
- Modify: `crates/panel/static/app.js`

**Interfaces:**
- Consumes: `GET /api/servers/:id/subusers`, `POST /api/servers/:id/subusers`, `DELETE /api/servers/:id/subusers/:uid`
- Produces: Visão com listagem de subusuários e modal para adicionar subusuários com checklist de permissões granulares

- [ ] **Step 1: Adicionar visual de subusuários no HTML do Servidor**

Adicione em `crates/panel/static/index.html` na barra lateral (sob o painel de uso de recursos):
```html
                <div class="glass-panel" style="padding: 20px; margin-top: 20px;">
                    <h3>Subusuários</h3>
                    <div id="subusers-list" style="margin: 15px 0; display: flex; flex-direction: column; gap: 10px;">
                        <!-- Listado dinamicamente -->
                    </div>
                    <button id="btn-add-subuser" class="btn-secondary" style="width: 100%; font-size: 0.9rem; padding: 8px;">+ Adicionar Subuser</button>
                </div>
```

E no final do arquivo (abaixo dos outros modais), adicione o modal de criação de subusuários:
```html
    <!-- Modal Criar Subuser -->
    <div id="subuser-modal" class="glass-panel" style="display: none; position: fixed; top: 50%; left: 50%; transform: translate(-50%, -50%); width: 90%; max-width: 450px; padding: 30px; z-index: 1000;">
        <h2 style="margin-bottom: 20px;">Adicionar Subusuário</h2>
        <form id="create-subuser-form" style="display: flex; flex-direction: column; gap: 15px;">
            <div>
                <label>User ID</label>
                <input type="text" id="sub-user-id" required placeholder="Cole o UUID do usuário">
            </div>
            <div>
                <label>Permissões</label>
                <div style="display: flex; flex-direction: column; gap: 8px; max-height: 200px; overflow-y: auto; padding: 10px; background: var(--bg-secondary); border-radius: 6px; border: 1px solid var(--border-color);">
                    <label><input type="checkbox" value="control.start"> control.start</label>
                    <label><input type="checkbox" value="control.stop"> control.stop</label>
                    <label><input type="checkbox" value="control.restart"> control.restart</label>
                    <label><input type="checkbox" value="control.console"> control.console</label>
                    <label><input type="checkbox" value="user.create"> user.create</label>
                    <label><input type="checkbox" value="user.read"> user.read</label>
                    <label><input type="checkbox" value="user.update"> user.update</label>
                    <label><input type="checkbox" value="user.delete"> user.delete</label>
                </div>
            </div>
            <div style="display: flex; gap: 10px; justify-content: flex-end;">
                <button type="button" id="close-sub-modal-btn" class="btn-secondary">Cancelar</button>
                <button type="submit" class="btn-primary">Adicionar</button>
            </div>
        </form>
    </div>
```

- [ ] **Step 2: Lógica de gerenciamento de subusuários em `app.js`**

Adicione em `crates/panel/static/app.js` no escopo global e estenda `loadServerView` para carregar subusuários:
No escopo global:
```javascript
const subModal = document.getElementById("subuser-modal");
document.getElementById("btn-add-subuser").onclick = () => subModal.style.display = "block";
document.getElementById("close-sub-modal-btn").onclick = () => subModal.style.display = "none";
```

Adicione ao final de `loadServerView`:
```javascript
        loadSubusers(id);
        
        document.getElementById("create-subuser-form").onsubmit = async (e) => {
            e.preventDefault();
            const checkedPerms = Array.from(document.querySelectorAll("#create-subuser-form input[type='checkbox']:checked")).map(el => el.value);
            const payload = {
                user_id: document.getElementById("sub-user-id").value,
                permissions: checkedPerms
            };
            try {
                await apiFetch(`/api/servers/${id}/subusers`, {
                    method: "POST",
                    body: JSON.stringify(payload)
                });
                subModal.style.display = "none";
                loadSubusers(id);
            } catch (err) {
                alert("Erro ao adicionar: " + err.message);
            }
        };
```

Adicione la função `loadSubusers`:
```javascript
async function loadSubusers(serverId) {
    const listEl = document.getElementById("subusers-list");
    listEl.innerHTML = "<p>Carregando...</p>";
    try {
        const subusers = await apiFetch(`/api/servers/${serverId}/subusers`);
        listEl.innerHTML = "";
        if (subusers.length === 0) {
            listEl.innerHTML = "<p style='font-size: 0.85rem; color: var(--text-secondary)'>Nenhum subuser configurado</p>";
            return;
        }

        subusers.forEach(su => {
            const card = document.createElement("div");
            card.style.display = "flex";
            card.style.justifyContent = "space-between";
            card.style.alignItems = "center";
            card.style.background = "var(--bg-secondary)";
            card.style.padding = "10px";
            card.style.borderRadius = "6px";
            card.style.fontSize = "0.85rem";
            card.innerHTML = `
                <div>
                    <p style="font-weight: 600;">Subuser ID: ${su.user_id.slice(0, 8)}...</p>
                    <p style="font-size: 0.75rem; color: var(--text-secondary)">${su.permissions.join(", ")}</p>
                </div>
                <button class="btn-delete-su" style="background: transparent; color: var(--error); padding: 4px; font-weight: bold;">✕</button>
            `;
            card.querySelector(".btn-delete-su").onclick = async () => {
                if (confirm("Remover acesso do subusuário?")) {
                    try {
                        await apiFetch(`/api/servers/${serverId}/subusers/${su.id}`, { method: "DELETE" });
                        loadSubusers(serverId);
                    } catch (err) {
                        alert("Erro: " + err.message);
                    }
                }
            };
            listEl.appendChild(card);
        });
    } catch (e) {
        listEl.innerHTML = `<p style="color: var(--error)">Erro ao carregar</p>`;
    }
}
```

- [ ] **Step 3: Testar subusuários**
Adicione um subusuário à sua conta e em seguida remova-o.
Expected: Registros persistem e removem dinamicamente no banco PostgreSQL sem recarregar a página inteira.

- [ ] **Step 4: Commit**

```bash
git add crates/panel/static/index.html crates/panel/static/app.js
git commit -m "feat(subusers): subuser management controls with permission checklists"
```
