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
| M4 Indexes | Done | `CREATE INDEX`, `WHERE col = literal` |
| M5 Compat test subset | Done | JSON fixtures in `crates/rusql-server/compat/` |
| M6 Auth + DROP/DELETE | Done | `--auth-password`; see [adr-m6-auth-and-dml.md](docs/en/specs/adr-m6-auth-and-dml.md) |
| M7 caching_sha2 | Done | Default auth plugin; [adr-m7-caching-sha2.md](docs/en/specs/adr-m7-caching-sha2.md) |
| M8 UPDATE | Done | `UPDATE … SET … WHERE` |
| M9 Transactions | Done | `BEGIN` / `COMMIT` / `ROLLBACK` |
| M10 SHOW TABLES | Done | `SHOW TABLES`, `SHOW DATABASES` |
| M11 Prepared statements | Done | `COM_STMT_PREPARE` / `EXECUTE` / `CLOSE` |
| M12 DESCRIBE / information_schema | Done | `DESCRIBE`, `SHOW COLUMNS`, `information_schema.tables/columns` |
| M13 SHOW CREATE TABLE | Done | MySQL-style DDL export |
| M14 SELECT projection | Done | `SELECT col1, col2 FROM …` |
| M15 USE database | Done | `USE rusql` |
| M16 SELECT LIMIT | Done | `LIMIT n` on table SELECT |
| M17 ORDER BY | Done | `ORDER BY col [ASC|DESC]` |
| M18 column aliases | Done | `SELECT col AS alias` |
| M19 LIMIT OFFSET | Done | `LIMIT n OFFSET m` |
| M20 WHERE ops | Done | `<`, `>`, `<=`, `>=`, `<>`, `AND` |
| M21 IS NULL | Done | `IS NULL` / `IS NOT NULL` |
| M22 INNER JOIN | Done | two-table `INNER JOIN ... ON` |
| M23 PRIMARY KEY | Done | `PRIMARY KEY` / `NOT NULL` in DESCRIBE |
| M24 ALTER ADD COLUMN | Done | `ALTER TABLE … ADD COLUMN` |
| M25 Binary resultset | Done | `COM_STMT_EXECUTE` binary rows |
| M26 caching_sha2 RSA | Done | Full RSA auth when `--auth-password` |
| M27 info_schema++ | Done | `SCHEMATA`, `STATISTICS` |
| M28 SHOW INDEX | Done | `SHOW INDEX FROM tbl` |
| M29+ | Planned | See [compat roadmap](docs/en/specs/mysql-compat-roadmap.md) |

**Roadmap**: [mysql-compat-roadmap.md](docs/en/specs/mysql-compat-roadmap.md) · **Book**: [docs/book/README.md](docs/book/README.md)

## Quick Start

```bash
cargo build --release
cargo run -p rusql-server -- --port 3307 --data-dir ./rusql-data
```

Data is persisted to `rusql-data/rusql.wal` and survives server restarts.

With MySQL client:

```bash
mysql -h 127.0.0.1 -P 3307 -u root --protocol=TCP
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
