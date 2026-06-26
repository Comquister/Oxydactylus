# Task 6 Report: check_server_access helper + access control on start/stop/command/stats/logs

## Status
**DONE**

## Commits Created
- **a80afda** - `feat(servers): check_server_access + permission enforcement on start/stop/command/stats/logs`

## Overview of Changes

### 1. `check_server_access` Helper
Implemented the access control helper function in `crates/panel/src/servers.rs`:
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

### 2. Handler Access Control Enforcement
Updated the following handlers in `crates/panel/src/servers.rs` to extract `user: AuthUser` instead of `_user: AuthUser` and call the access check:
- `start_server`: Enforces `CONTROL_START` (`control.start`).
- `stop_server`: Enforces `CONTROL_STOP` (`control.stop`).
- `server_command`: Enforces `CONTROL_CONSOLE` (`control.console`).
- `server_stats`: Enforces general subuser access (passing `None` as the permission parameter, verifying that the user has at least one permission or is the owner/admin).
- `stream_server_logs`: Enforces `CONTROL_CONSOLE` (`control.console`).

### 3. Simplify Database Fetches
Refactored and simplified server lookup in other handlers to eliminate redundant SELECT queries, replacing them with the shared `fetch_server` helper:
- `delete_server`
- `provision_server`

---

## TDD Evidence

### RED Phase (Failures before implementation)
We appended three new tests to the test suite in `crates/panel/src/servers.rs`:
1. `subuser_with_start_can_start_server` (Passed initially because no permissions checks were in place).
2. `subuser_without_stop_gets_403` (FAILED: assertion failed `left == right` where left was `204` and right was `403`).
3. `stranger_gets_403_on_start` (FAILED: assertion failed `left == right` where left was `204` and right was `403`).

```
running 1 test
test servers::tests::subuser_without_stop_gets_403 ... FAILED

failures:

---- servers::tests::subuser_without_stop_gets_403 stdout ----
thread 'servers::tests::subuser_without_stop_gets_403' panicked at crates/panel/src/servers.rs:1063:9:
assertion `left == right` failed
  left: 204
 right: 403
```

```
running 1 test
test servers::tests::stranger_gets_403_on_start ... FAILED

failures:

---- servers::tests::stranger_gets_403_on_start stdout ----
thread 'servers::tests::stranger_gets_403_on_start' panicked at crates/panel/src/servers.rs:1093:9:
assertion `left == right` failed
  left: 204
 right: 403
```

### GREEN Phase (Passing after implementation)
Once `check_server_access` and the handler-level checks were implemented, all tests passed:
```
running 68 tests
...
test servers::tests::subuser_with_start_can_start_server ... ok
test servers::tests::stranger_gets_403_on_start ... ok
test servers::tests::subuser_without_stop_gets_403 ... ok
...
test result: ok. 68 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.91s
```

All other workspace tests (oxy-core, oxy-node, oxy-panel integration tests) are fully passing.

## Concerns
None. Everything is clean and compiles smoothly with zero warning changes.
