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

### Transactions (M9)

```sql
BEGIN;
INSERT INTO users VALUES (2, 'bob');
SELECT * FROM users;
COMMIT;
```

Uncommitted changes are not visible to other connections. `ROLLBACK` discards the current transaction.

### Query SQL (M22–M46)

```sql
-- JOINs including OUTER (M41)
SELECT a.name, b.label FROM a LEFT JOIN b ON a.id = b.a_id;

-- GROUP BY / HAVING (M43)
SELECT dept, COUNT(*) AS cnt FROM emp GROUP BY dept HAVING cnt > 1;

-- Subqueries (M42)
SELECT id FROM t WHERE id IN (SELECT ref_id FROM refs);
SELECT id FROM t WHERE EXISTS (SELECT 1 FROM refs r WHERE r.t_id = t.id);
SELECT id, val FROM (SELECT id, val FROM t) AS d;

-- Expressions (M46)
SELECT id + 1, CONCAT(name, '!'), COALESCE(note, 'n/a'), LOWER(name) FROM t;

-- UNION (M44)
SELECT id FROM a UNION SELECT id FROM b;
SELECT id FROM a UNION ALL SELECT id FROM b;

-- FOREIGN KEY (M39)
CREATE TABLE parent (id INT PRIMARY KEY);
CREATE TABLE child (
  id INT PRIMARY KEY,
  parent_id INT,
  CONSTRAINT fk_child_parent FOREIGN KEY (parent_id) REFERENCES parent (id)
);
SELECT * FROM information_schema.KEY_COLUMN_USAGE WHERE TABLE_NAME = 'child';

-- GRANT / REVOKE (M54)
GRANT SELECT, INSERT ON rusql.* TO app;
SHOW GRANTS FOR app;
REVOKE INSERT ON rusql.* FROM app;

-- Composite indexes (M50)
CREATE INDEX idx_ab ON t (a, b);
SELECT * FROM t WHERE a = 1 AND b = 2;
SHOW INDEX FROM t;

-- Multi-user auth (M55-auth)
CREATE USER 'app'@'%' IDENTIFIED BY 'secret';
CREATE USER 'legacy'@'%' IDENTIFIED WITH mysql_native_password BY 'secret';
DROP USER 'legacy'@'%';
```

Extended types (M40): `DECIMAL(p,s)`, `DATETIME`, `TEXT`, `BLOB`, `JSON` in `CREATE TABLE` and `DESCRIBE`.

Run the mysql-test wire subset (M60):

```bash
node scripts/mysql-test-subset.mjs
cargo test -p rusql-server mysql_test_subset
```

### Schema discovery (M10–M12)

```sql
SHOW TABLES;
SHOW DATABASES;
USE rusql;
DESCRIBE users;
SHOW COLUMNS FROM users;
SELECT * FROM information_schema.tables;
SELECT * FROM information_schema.columns WHERE table_name = 'users';
SHOW CREATE TABLE users;
```

### Prepared statements (M11)

Use MySQL client or driver with prepared statements; rusql supports `COM_STMT_*` with `?` binding and binary resultset rows on execute.

```bash
cargo test -p rusql-server stmt_prepare_execute
```

## Release history

See [release-notes.md](release-notes.md) and [CHANGELOG.md](../../CHANGELOG.md) at the repo root (updated on every merged PR).

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
| SELECT column list | Done | M14 `SELECT id, name FROM …` |
| ORDER BY | Done | M17 `ORDER BY col [ASC|DESC]` |
| Column aliases | Done | M18 `SELECT col AS alias` |
| LIMIT | Done | M16 `LIMIT n` |
| OFFSET | Done | M19 `LIMIT n OFFSET m` |
| SELECT literal | Done | e.g. `SELECT 1` |
| Persistence (WAL) | Done | `--data-dir`, file `rusql.wal` |
| Prepared statements | Done | `COM_STMT_PREPARE` / `EXECUTE` / `CLOSE`; binary resultset on execute (M25) |
| COM_CHANGE_USER / COM_RESET_CONNECTION | Done | M51 re-auth; reset clears prepared state |
| COM_FIELD_LIST / stmt long data | Done | M52 legacy field list; `COM_STMT_SEND_LONG_DATA` + `COM_STMT_RESET` |
| SHOW PROCESSLIST / COM_PROCESS_INFO | Done | M53 active connection registry |
| Transactions | Done | `BEGIN` / `COMMIT` / `ROLLBACK`; see [m9-transactions.md](specs/m9-transactions.md) |
| SHOW TABLES / DATABASES | Done | M10 schema discovery |
| DESCRIBE / information_schema | Done | M12; [m12-describe-info-schema.md](specs/m12-describe-info-schema.md) |
| SHOW CREATE TABLE | Done | M13 schema export DDL |
| ALTER TABLE ADD COLUMN | Done | M24 schema evolution |
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

## Stored programs and replication (P3 MVP)

- **Procedures / triggers / functions**: `CREATE PROCEDURE … BEGIN … END`, `CALL proc()`, `CREATE FUNCTION … RETURNS … BEGIN RETURN … END` (scalar in `SELECT`), `CREATE TRIGGER` (BEFORE INSERT with `SET NEW.col`; AFTER UPDATE/DELETE with `OLD.col`/`NEW.col` in DML body), `DROP PROCEDURE` / `DROP FUNCTION` / `DROP TRIGGER`. Metadata persists in `{data_dir}/programs.json`.
- **Catalog views**: `SELECT * FROM information_schema.ROUTINES` and `information_schema.TRIGGERS`.
- **Binlog on COMMIT**: Transaction commits append QUERY events to `{data_dir}/binlog/binlog.NNNNNN` with GTID comment prefix.
- **Replication stubs**: `COM_BINLOG_DUMP` streams binlog bytes; `COM_REGISTER_SLAVE` returns OK. `SHOW MASTER STATUS` / `SHOW SLAVE STATUS` return MVP rows.

See [adr-replication.md](specs/adr-replication.md).

## Development sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
node scripts/doc-parity.mjs
node scripts/check-changelog.mjs
node scripts/metrics.mjs
```

## Performance benchmark (PERF-B1)

Persistent-connection micro-benchmark (same 7 workloads as [performance-benchmark-2026-08-11.md](reports/performance-benchmark-2026-08-11.md), without per-query CLI spawn):

```bash
cargo build --release -p rusql-server
cargo run -p rusql-server -- --port 3307 --data-dir ./.test-data-bench

node scripts/bench-rusql-vs-mysql.mjs --host 127.0.0.1 --port 3307 --label rusql \
  --output target/bench-rusql.json

node scripts/bench-rusql-vs-mysql.mjs --compare --rusql-port 3307 --mysql-port 3308
```

JSON output includes QPS and p50/p95 latency per workload plus host/platform metadata. Local artifacts: `target/bench-*.json` (gitignored).

### Collation (M59)

String `ORDER BY` and `WHERE =` use `utf8mb4_unicode_ci` (case/accent insensitive, ß→ss). Verify:

```bash
cargo test -p rusql-core collation
cargo test -p rusql-executor collation
```

```sql
SHOW COLLATION;
SELECT * FROM information_schema.columns WHERE table_name = 'users';
```

Supported collations: `utf8mb4_unicode_ci` (default).

### Sysbench comparison (M61 / PERF-B6)

Industry-standard `oltp_point_select` against rusql and Docker MySQL 8.0:

```bash
sudo apt-get install -y sysbench   # or choco install sysbench on Windows
docker run -d --name rusql-mysql80-bench -e MYSQL_ALLOW_EMPTY_PASSWORD=yes -p 3308:3306 mysql:8.0
cargo run -p rusql-server -- --port 3307 --data-dir ./.test-data-sysbench
node scripts/sysbench-rusql.mjs --rusql-port 3307 --mysql-port 3308 --threshold 0.7
```

Soft-fails when Sysbench or Docker are missing. CI: `.github/workflows/sysbench.yml` (`workflow_dispatch`).

## Performance optimizations (PERF-B2 / PERF-B3)

- **`SELECT … ORDER BY indexed_col LIMIT n`** (no `WHERE`): secondary-index ordered scan with early stop.
- **`UPDATE … WHERE pk = ?`**: PK index lookup + incremental index maintenance.

```bash
cargo test -p rusql-storage scan_index_ordered_with_limit pk_update_without_index_rebuild
cargo test -p rusql-executor select_order_by_indexed_limit update_pk_by_index
```

### Multi-threaded benchmark (PERF-B4)

```bash
node scripts/bench-rusql-vs-mysql.mjs --threads 8 --duration 30 --workloads read-heavy \
  --host 127.0.0.1 --port 3307 --label rusql
node scripts/bench-rusql-vs-mysql.mjs --thread-matrix --compare \
  --rusql-port 3307 --mysql-port 3308 --duration 10
```

### WAL sync policy (PERF-B5)

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./.test-data-bench
cargo run -p rusql-server -- --wal-sync batch --port 3307 --data-dir ./.test-data-bench
cargo run -p rusql-server -- --wal-sync none --port 3307 --data-dir ./.test-data-bench
```

**Warning**: `batch` and `none` trade durability for speed.
