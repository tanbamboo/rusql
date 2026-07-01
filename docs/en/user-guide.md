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

### Optional password verification

By default, any client password is accepted (dev mode). To enable verification (`caching_sha2_password` + `mysql_native_password`):

```bash
cargo run -p rusql-server -- --port 3307 --auth-password your_secret
```

Handshake advertises `caching_sha2_password` (MySQL 8 default). Legacy clients may still use `mysql_native_password`. See [adr-m7-caching-sha2.md](specs/adr-m7-caching-sha2.md).

## Automated tests (recommended)

Runs handshake + SQL over the wire without external tools:

```bash
cargo test -p rusql-server com_query
cargo test -p rusql-server compat
cargo test -p rusql-protocol
cargo test
```

### Compatibility fixture suite (M5)

JSON fixtures under `crates/rusql-server/compat/` drive end-to-end wire tests (CREATE/INSERT/SELECT/INDEX/WHERE). Add new cases by editing `basic.json` and running:

```bash
cargo test -p rusql-server run_basic_compat_fixtures
```

## Manual test with MySQL client

After starting the server on port 3307:

```bash
mysql -h 127.0.0.1 -P 3307 -u root --protocol=TCP
```

If your client defaults to `caching_sha2_password`, force native password (see [adr-auth-mvp.md](specs/adr-auth-mvp.md)):

```bash
mysql -h 127.0.0.1 -P 3307 -u root --protocol=TCP
```

MySQL 8 clients use `caching_sha2_password` by default. If needed:

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

## Implemented features (M1–M6)

| Feature | Status | Notes |
|---------|--------|-------|
| MySQL wire protocol v10 handshake | Done | Default `caching_sha2_password`; native fallback |
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
| Compat fixture suite | Done | `cargo test -p rusql-server compat` |
| DROP TABLE | Done | |
| DELETE | Done | `WHERE col = literal` or all rows |
| UPDATE | Done | `SET col = literal` with optional `WHERE` |

## Troubleshooting

| Problem | Solution |
|---------|----------|
| Connection refused | Check server is running and port matches |
| Auth plugin error | Try without `--default-auth`; or use `mysql_native_password` |
| SQL syntax error | See [adr-sql-parser.md](specs/adr-sql-parser.md); we use `sqlparser` MySQL dialect |

## Development sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
```
