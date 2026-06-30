# Rust Profile — Guides (Feedforward Control)

## Runtime

- Rust 2021 edition
- Cargo workspace at repository root
- Async runtime: **Tokio**
- Networking: `tokio::net` for MySQL wire protocol (not HTTP/axum for core server)

## Crate Layout

```
crates/
├── rusql-i18n/       # User-visible messages (en-US, zh-CN)
├── rusql-protocol/   # MySQL packet encode/decode, handshake
├── rusql-sql/        # SQL parse (sqlparser MySQL dialect)
├── rusql-core/       # Catalog, session, types
├── rusql-storage/    # StorageEngine trait + implementations
├── rusql-executor/   # Query execution
├── rusql-planner/    # Query planning (pass-through MVP)
├── rusql-server/     # TCP listener binary
└── rusql-cli/        # Admin CLI
```

## Coding Conventions

- Public APIs require `///` doc comments (English)
- Errors: `thiserror` in libraries, `anyhow` in binaries; no bare `unwrap()`/`expect()` on production paths
- Async: `async fn` + `.await`; use `spawn_blocking` for blocking I/O
- Naming: `snake_case` functions/vars, `PascalCase` types
- Logging: `tracing` structured logs; no `println!` in library code

## i18n

- All user-visible strings use `rusql_i18n::t!("key")` or `t!` macro
- Locale files: `crates/rusql-i18n/locales/en-US.yml`, `zh-CN.yml`
- Default: `en-US`; override via `RUSQL_LOCALE` env or `--locale` CLI flag

## Architecture

- `rusql-protocol` does not depend on `rusql-executor` or `rusql-storage`
- `rusql-sql` is parser-only; execution lives in `rusql-executor`
- Business logic in services/modules; thin protocol handlers
- Integration tests in `tests/` directories or `#[cfg(test)]` modules

## Forbidden

- `unsafe` without ADR approval
- Hardcoded English or Chinese user strings in `crates/` (use i18n keys)
- Ignoring `cargo clippy` warnings
- `clone()` to avoid borrow checker refactoring on hot paths

## Pre-Completion Checklist

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
```
