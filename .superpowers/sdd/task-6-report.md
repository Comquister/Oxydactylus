# Task 6 Report — CLI Entrypoint + Integration Smoke Tests

## Summary

Created the `oxydactylus` CLI binary (`crates/cli/`) and `config.example.toml`.

### Files Created / Modified

- `crates/cli/Cargo.toml` — added `[[bin]]` section and all required dependencies (clap pinned at `"4"`, workspace deps for tokio/anyhow/tracing/tracing-subscriber/toml, path deps for oxy-core/oxy-node/oxy-panel)
- `crates/cli/src/main.rs` — full implementation with clap `Parser` struct, config reading, role dispatch with `tokio::join!` for `Role::Both`
- `config.example.toml` — example config at workspace root covering all three roles

### Build

```
cargo build
Finished `dev` profile [unoptimized + debuginfo] target(s) in 53.69s
```

Binary at `target/debug/oxydactylus`.

---

## Smoke Test Results

### Step 5: `--help` shows usage with `-c`/`--config` option

```
$ ./target/debug/oxydactylus --help
Game server management panel

Usage: oxydactylus [OPTIONS]

Options:
  -c, --config <CONFIG>  [default: config.toml]
  -h, --help             Print help
  -V, --version          Print version
```

**PASS** ✓

---

### Step 6: Missing config gives clear error

```
$ ./target/debug/oxydactylus --config /nonexistent 2>&1
Error: cannot read "/nonexistent": No such file or directory (os error 2)
```

**PASS** ✓

---

### Step 7: Missing `[panel]` section with `role=panel` gives clear error

```
$ printf '[role]\ntype = "panel"\n' > /tmp/bad.toml
$ ./target/debug/oxydactylus --config /tmp/bad.toml 2>&1
Error: [panel] section required when role = "panel"
```

**PASS** ✓

---

### Step 8: `role=both` logs both "panel starting" and "node starting"

```
$ cp config.example.toml config.toml
$ RUST_LOG=info timeout 2 ./target/debug/oxydactylus 2>&1 || true
2026-06-24T17:13:47.524846Z  INFO oxy_panel: panel starting listen=0.0.0.0:3000
2026-06-24T17:13:47.524886Z  INFO oxy_node: node starting listen=0.0.0.0:8080
```

Both services started concurrently via `tokio::join!` and ran until `timeout 2` killed the process.

**PASS** ✓

---

## Implementation Notes

- `Role::Both` uses `tokio::join!` so both futures run concurrently; errors from each are propagated after both complete.
- No `println!` anywhere — all output via `tracing`.
- Default config path is `config.toml`; overridable with `-c` / `--config`.
- `clap = { version = "4", features = ["derive"] }` pinned directly (not in workspace), per constraints.
- `config.toml` remains in `.gitignore` (added by Task 1); only `config.example.toml` is tracked.
