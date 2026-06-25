# Server Status Tracking — Design Spec

**Data:** 2026-06-25  
**Status:** Aprovado

## Objetivo

Expor e manter atualizado o campo `status` dos servidores. Atualmente o campo existe no banco desde a migração 004 mas nunca é retornado pela API nem atualizado pelos handlers de ciclo de vida. Após este plano, os clientes conseguem saber o estado real de cada servidor.

## Contexto

A migração 004 adicionou à tabela `servers`:

```sql
ADD COLUMN status TEXT NOT NULL DEFAULT 'stopped'
    CHECK (status IN ('installing', 'running', 'stopped', 'error'));
```

O struct `Server` em `crates/panel/src/servers.rs` não inclui o campo, todos os `SELECT` o omitem e o `INSERT` de `create_server` usa o default do banco sem significado semântico.

## Estados e Transições

| Handler | Antes da chamada gRPC | Sucesso | Falha |
|---|---|---|---|
| `create_server` | INSERT `status = 'installing'` | UPDATE `status = 'stopped'` | DELETE do registro (já existente) |
| `start_server` | — | UPDATE `status = 'running'` | UPDATE `status = 'error'` |
| `stop_server` | — | UPDATE `status = 'stopped'` | sem mudança |
| `delete_server` | — | registro deletado | sem mudança |

**Justificativas:**
- `create_server` usa `'installing'` porque `provision` é síncrono mas pode levar segundos; `'stopped'` sinalizaria que o servidor está pronto, o que seria falso antes do provision terminar.
- `start_server` é síncrono (gRPC aguarda o Docker responder), então `'running'`/`'error'` no retorno reflete o estado real.
- `stop_server` em falha mantém o status anterior: se o Docker não conseguiu parar o container, ele provavelmente ainda está `'running'`, não `'stopped'`.
- `delete_server` não precisa de transição — o registro é removido.

## Escopo

**Sem migrações** — a coluna já existe.  
**Sem novas rotas** — `GET /api/servers` e `GET /api/servers/:id` já existem; passam a incluir `status` na resposta.  
**Arquivo único** — todas as mudanças ficam em `crates/panel/src/servers.rs`.

## Mudanças em `servers.rs`

### Struct `Server`

Adicionar campo:

```rust
pub status: String,
```

### Todos os `SELECT`

Incluir `status` nas colunas retornadas:

```sql
SELECT id, node_id, name, image, memory_mb, cpu_percent, env, status, created_at
FROM servers ...
```

### `create_server` — INSERT + UPDATE pós-provision

```sql
-- INSERT explícito com 'installing'
INSERT INTO servers (node_id, name, image, memory_mb, cpu_percent, env, egg_id, status)
VALUES ($1, $2, $3, $4, $5, $6, $7, 'installing')
RETURNING id, node_id, name, image, memory_mb, cpu_percent, env, status, created_at

-- Após provision bem-sucedido:
UPDATE servers SET status = 'stopped' WHERE id = $1
```

Em caso de falha do provision: DELETE existente (sem mudança de comportamento).

### `start_server` — UPDATE pós-chamada gRPC

```sql
-- Sucesso:
UPDATE servers SET status = 'running' WHERE id = $1
-- Falha:
UPDATE servers SET status = 'error' WHERE id = $1
```

### `stop_server` — UPDATE pós-chamada gRPC

```sql
-- Sucesso:
UPDATE servers SET status = 'stopped' WHERE id = $1
-- Falha: sem UPDATE
```

## Testes

Os testes existentes em `servers.rs` precisam ser atualizados para incluir `status` nas queries de seed direto no banco (ex: `create_server_provisions_on_node` já usa `INSERT INTO servers` sem `status` — continuará funcionando pelo default do banco). Os novos comportamentos a testar:

- `create_server` retorna servidor com `status = 'stopped'` após provision bem-sucedido
- `start_server` atualiza `status` para `'running'` no banco
- `stop_server` atualiza `status` para `'stopped'` no banco
- `start_server` com nó falhando atualiza `status` para `'error'`

## Restrições Globais

- Rust edition 2021, workspace resolver = "2"
- Sem migrações novas — coluna já existe
- Testes usam `#[sqlx::test(migrations = "./migrations")]`
- Zero regressões nos testes existentes
- YAGNI: apenas o que está neste spec

## Detalhe: resposta de `create_server`

O `RETURNING` do INSERT devolve o servidor com `status = 'installing'`. Após o UPDATE de provision bem-sucedido, o handler deve retornar o servidor com `status = 'stopped'`. Implementar via `UPDATE servers SET status = 'stopped' WHERE id = $1 RETURNING id, node_id, name, image, memory_mb, cpu_percent, env, status, created_at` e substituir o `server` local pelo resultado, ou simplesmente setar `server.status = "stopped".to_string()` em memória após o UPDATE (ambas abordagens são aceitáveis; a segunda é mais simples).
