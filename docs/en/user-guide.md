# User Guide — Testing rusql

This guide describes **what works today** on `main` and how to verify it.

## Prerequisites

- Rust 1.75+ ([rustup](https://rustup.rs))
- Optional: MySQL client (`mysql` CLI) for manual testing

## Build

```bash
cargo build --release
```

## Run the server

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./rusql-data
```

- `--data-dir` — directory for the WAL file (`rusql.wal`). Default: `rusql-data`
- Data **survives restarts**: stop the server, start again, tables and rows are replayed from WAL

Default locale is `en-US`. For Chinese messages:

```bash
RUSQL_LOCALE=zh-CN cargo run -p rusql-server -- --port 3307
```

## Automated tests (recommended)

Runs handshake + SQL over the wire without external tools:

```bash
cargo test -p rusql-server com_query
cargo test -p rusql-protocol
cargo test
```

## Manual test with MySQL client

After starting the server on port 3307:

```bash
mysql -h 127.0.0.1 -P 3307 -u root --protocol=TCP
```

If your client defaults to `caching_sha2_password`, force native password (see [adr-auth-mvp.md](specs/adr-auth-mvp.md)):

```bash
mysql -h 127.0.0.1 -P 3307 -u root --default-auth=mysql_native_password --protocol=TCP
```

Example session:

```sql
CREATE TABLE users (id INT, name VARCHAR(64));
CREATE INDEX idx_users_id ON users (id);
INSERT INTO users VALUES (1, 'alice');
SELECT * FROM users WHERE id = 1;
quit
```

Restart the server and run `SELECT * FROM users WHERE id = 1;` again — rows are still present.

## Persistence test (automated)

```bash
cargo test -p rusql-server persistence_across_connections
```

## Implemented features (M1–M4)

| Feature | Status | Notes |
|---------|--------|-------|
| MySQL wire protocol v10 handshake | Done | `mysql_native_password` stub (no hash verify yet) |
| COM_QUERY | Done | Single-statement queries |
| COM_QUIT | Done | |
| CREATE TABLE | Done | Column types stored as metadata |
| INSERT … VALUES | Done | |
| SELECT * FROM table | Done | |
| SELECT literal | Done | e.g. `SELECT 1` |
| Persistence (WAL) | Done | `--data-dir`, file `rusql.wal` |
| Prepared statements | Not yet | |
| Transactions | Not yet | |
| Indexes | Done | `CREATE INDEX`, point lookup via `WHERE col = literal` |

## Troubleshooting

| Problem | Solution |
|---------|----------|
| Connection refused | Check server is running and port matches |
| Auth plugin error | Use `--default-auth=mysql_native_password` |
| SQL syntax error | See [adr-sql-parser.md](specs/adr-sql-parser.md); we use `sqlparser` MySQL dialect |

## Development sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
```
