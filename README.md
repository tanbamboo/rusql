# rusql

A MySQL 8.0-compatible database written in Rust, built with [Harness Engineering](https://martinfowler.com/articles/harness-engineering.html) for AI-native development.

**简体中文**: [docs/zh-CN/README.md](docs/zh-CN/README.md)

## Status

Active development toward MySQL 8.0 compatibility.

| Milestone | Status |
|-----------|--------|
| M0 Harness | Done |
| M1 Wire protocol handshake | Done |
| M2 COM_QUERY (CREATE/INSERT/SELECT) | Done |
| M3 WAL persistence | Done | `--data-dir` (default `rusql-data`) |
| M4 Indexes | Planned |
| M5 Compat test subset | Planned |
| M6+ Replication, views, procedures | Planned |

**Test what's implemented today**: [docs/en/user-guide.md](docs/en/user-guide.md)

## Quick Start

```bash
cargo build --release
cargo run -p rusql-server -- --port 3307 --data-dir ./rusql-data
```

Data is persisted to `rusql-data/rusql.wal` and survives server restarts.

With MySQL client:

```bash
mysql -h 127.0.0.1 -P 3307 -u root --default-auth=mysql_native_password --protocol=TCP
```

## Architecture

```
crates/
├── rusql-i18n       # Internationalization (en-US, zh-CN)
├── rusql-protocol   # MySQL wire protocol
├── rusql-sql        # SQL parsing (sqlparser)
├── rusql-core       # Catalog, session, types
├── rusql-storage    # Storage engine trait + implementations
├── rusql-executor   # Query executor
├── rusql-planner    # Query planner (pass-through MVP)
├── rusql-server     # TCP server binary
└── rusql-cli        # Admin CLI
```

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
```

See [AGENTS.md](AGENTS.md) and [docs/en/workflows/spec-to-ship.md](docs/en/workflows/spec-to-ship.md).

## License

Apache-2.0
