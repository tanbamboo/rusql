# Release Notes

What landed on `main` and how to verify it. For day-to-day usage see [user-guide.md](user-guide.md).

**中文**: [release-notes.md](../zh-CN/release-notes.md)

---

## Latest: P3 stored programs + replication MVP (2026-09-01)

**What**: `CREATE PROCEDURE` / `CALL` / `DROP PROCEDURE`, `CREATE TRIGGER` (BEFORE INSERT with `SET NEW.col`), binlog QUERY events on `COMMIT` with GTID comment stub, `COM_BINLOG_DUMP` / `COM_REGISTER_SLAVE`, `apply_binlog_file` replica applier, `SHOW MASTER/SLAVE STATUS` GTID stubs.

```bash
cargo test -p rusql-sql stored_programs
cargo test -p rusql-executor programs
cargo test -p rusql-storage binlog
cargo test -p rusql-storage replica
```

---

## Latest: M51–M53 wire protocol commands (2026-09-01)

**What**: `COM_CHANGE_USER`, `COM_RESET_CONNECTION`, `COM_FIELD_LIST`, prepared-statement long data/reset, and `SHOW PROCESSLIST` / `COM_PROCESS_INFO`.

```bash
cargo test -p rusql-protocol
cargo test -p rusql-server show_processlist
cargo test -p rusql-server com_field_list
cargo test -p rusql-server com_change_user
```

---

## Latest: PERF-B1 persistent-connection benchmark (2026-09-01)

**What**: `scripts/bench-rusql-vs-mysql.mjs` runs the same 7 workloads as the 2026-08-11 CLI baseline using one persistent wire client (no per-query process spawn).

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./.test-data-bench
node scripts/bench-rusql-vs-mysql.mjs --host 127.0.0.1 --port 3307 --label rusql --output target/bench-rusql.json
```

---

## Latest: M55-auth multi-user accounts (2026-09-01)

**What**: `CREATE USER` / `DROP USER` with passwords persisted in `mysql.user.json`; login as non-root users via `caching_sha2_password` or `mysql_native_password`.

```bash
cargo test -p rusql-core parse_create_user_ddl
cargo test -p rusql-server auth
```

**Try it** (dev server, then as root):

```sql
CREATE USER 'app'@'%' IDENTIFIED WITH mysql_native_password BY 'secret';
```

Connect with `--default-auth=mysql_native_password` and `-u app -p`.

---

## M50 composite indexes (2026-09-01)

**What**: Multi-column `CREATE INDEX idx ON t (a, b)` with prefix equality lookups, composite EXPLAIN plans, and MySQL-style `Seq_in_index` in SHOW INDEX / `information_schema.STATISTICS`.

```bash
cargo test -p rusql-storage composite_index_lookup
cargo test -p rusql-planner composite_eq
cargo test -p rusql-server run_basic_compat
```

---

## M54 GRANT/REVOKE (2026-08-31)

**What**: MySQL-style privilege grants persisted to `mysql.user.json` in the data directory; `GRANT`/`REVOKE`/`SHOW GRANTS`; unauthorized DML returns errno **1142**. User `root` bypasses all checks.

```bash
cargo test -p rusql-core privileges
cargo test -p rusql-server run_basic_compat
```

Example:

```sql
GRANT SELECT, INSERT ON rusql.* TO app;
SHOW GRANTS FOR app;
REVOKE INSERT ON rusql.* FROM app;
```

---

## M39 FOREIGN KEY + M44 UNION (2026-08-31)

**What**: `UNION`/`UNION ALL` result combination; `FOREIGN KEY` on `CREATE TABLE` with RESTRICT enforcement (MySQL errno 1451/1452) and `information_schema.KEY_COLUMN_USAGE`.

```bash
cargo test -p rusql-executor foreign_key
cargo test -p rusql-server compat_suite
node scripts/mysql-diff.mjs   # requires Docker
```

---

## Latest: P1 SQL gaps M40–M60 (2026-08-31)

**What**: Extended column types, outer joins, GROUP BY/HAVING/aggregates, subqueries, SQL expressions, and 100-case mysql-test wire harness with CI pass floor.

```bash
cargo test -p rusql-executor aggregate
cargo test -p rusql-server mysql_test_subset
node scripts/mysql-diff.mjs   # requires Docker
```

---

## Latest: Issue #73 — metadata EOF + SESSION_TRACK (2026-07-06)

**What**: Completes MySQL 8.0 CLI compat after PR #78. Text/binary resultsets now send metadata EOF/OK between column definitions and rows (#79). OK packets include an empty session-state trailer when `CLIENT_SESSION_TRACK` is negotiated (#80). Command-phase OK responses honor client capabilities.

```bash
cargo test -p rusql-protocol response::tests
cargo test -p rusql-server mysql_cli
node scripts/mysql-diff.mjs   # requires Docker; CI uses apt mysql client
```

---

## Issue #73 — MySQL 8.0 CLI COM_QUERY compat (2026-07-06)

**What**: Official `mysql:8.0` clients negotiate `CLIENT_QUERY_ATTRIBUTES` and `CLIENT_DEPRECATE_EOF`. rusql now strips the query-attributes preamble on `COM_QUERY` and ends text/binary resultsets with an OK packet instead of a legacy EOF when required.

```bash
cargo test -p rusql-protocol command::tests
cargo test -p rusql-server mysql_cli_query_attributes
node scripts/mysql-diff.mjs   # requires Docker
```

---

## M31 — Durable COMMIT WAL (2026-06-30)

**What**: `COMMIT` appends pending transaction records to `rusql.wal` with `sync_data`; `ROLLBACK` discards overlay without WAL writes. Verified across storage replay and wire-protocol tests.

```bash
cargo test -p rusql-storage commit_transaction_survives
cargo test -p rusql-server transaction
```

---

## M30 — mysql-test subset (2026-06-30)

**What**: 12 Oracle mysql-test inspired wire cases in `tests/mysql-test/manifest.json`, run via internal test client. Skips documented in `tests/mysql-test/SKIPS.md`.

```bash
node scripts/mysql-test-subset.mjs
cargo test -p rusql-server mysql_test_subset
```

---

## M29 — mysql-diff runner (2026-06-30)

**What**: `node scripts/mysql-diff.mjs` compares portable SQL in `compat/mysql-diff.json` against Docker MySQL 8.0 and rusql-server (skips without Docker).

```bash
node scripts/mysql-diff.mjs
```

---

## M28 — SHOW INDEX (2026-06-30)

**What**: `SHOW INDEX FROM tbl` (also `SHOW INDEXES`, `SHOW KEYS`) lists PRIMARY and secondary indexes with MySQL column names.

```bash
cargo test -p rusql-sql show_index
cargo test -p rusql-executor show_index
cargo test -p rusql-server compat
```

---

## M27 — information_schema SCHEMATA & STATISTICS (2026-06-30)

**What**: `SELECT * FROM information_schema.SCHEMATA` and `STATISTICS` (PRIMARY + secondary indexes).

```bash
cargo test -p rusql-executor info_schema_schemata
cargo test -p rusql-server compat
```

---

## M26 — caching_sha2 RSA full auth (2026-06-30)

**What**: Non-TLS clients can complete `caching_sha2_password` via RSA public-key exchange when `--auth-password` is set.

```bash
cargo test -p rusql-server accepts_caching_sha2_rsa
cargo test -p rusql-protocol rsa_password_roundtrip
```

---

## M25 — Binary resultset (COM_STMT_EXECUTE) (2026-06-30)

**What**: Prepared-statement SELECT returns binary protocol rows with correct MySQL column types (`INT` as 4-byte LE, `VARCHAR` as lenenc string).

```bash
cargo test -p rusql-protocol binary
cargo test -p rusql-server stmt_prepare_execute_binary
```

---

## M24 — ALTER TABLE ADD COLUMN (2026-06-30)

**What**: `ALTER TABLE t ADD COLUMN c TYPE` (and MySQL shorthand `ADD c TYPE`); existing rows get NULL (empty string) in the new column; WAL replay.

```bash
cargo test -p rusql-executor alter_table_add_column
cargo test -p rusql-server compat
```

---

## M23 — PRIMARY KEY metadata (2026-06-30)

**What**: `PRIMARY KEY` and `NOT NULL` stored in catalog; shown in DESCRIBE / SHOW CREATE TABLE.

```bash
cargo test -p rusql-executor describe_primary_key
cargo test -p rusql-server compat
```

---

## M22 — INNER JOIN (2026-06-30)

**What**: `SELECT ... FROM a INNER JOIN b ON a.col = b.col` (two tables).

```bash
cargo test -p rusql-executor inner_join_two_tables
cargo test -p rusql-server compat
```

---

## M21 — IS NULL / IS NOT NULL (2026-06-30)

**What**: `WHERE col IS NULL` and `IS NOT NULL`; `INSERT … NULL` supported.

```bash
cargo test -p rusql-executor where_is_null
cargo test -p rusql-server compat
```

---

## M20 — WHERE comparisons and AND (2026-06-30)

**What**: `WHERE id > 1`, `id <> 2`, `id = 1 AND name = 'x'` on table SELECT.

```bash
cargo test -p rusql-executor where_comparisons_and
cargo test -p rusql-server compat
```

---

## M19 — SELECT LIMIT OFFSET (2026-06-30)

**What**: `LIMIT n OFFSET m` after ORDER BY / projection on table SELECT.

```sql
SELECT * FROM users ORDER BY id LIMIT 1 OFFSET 1;
```

```bash
cargo test -p rusql-executor select_limit
cargo test -p rusql-server compat
```

---

## M18 — SELECT column aliases (2026-06-30)

**What**: Result set column headers use `AS` aliases (e.g. `SELECT id AS user_id`).

```sql
SELECT id AS user_id FROM users;
```

```bash
cargo test -p rusql-executor select_column_aliases
cargo test -p rusql-server compat
```

---

## M17 — SELECT ORDER BY (2026-06-30)

**What**: `ORDER BY col [ASC|DESC]` on table `SELECT` (after projection/filter, before `LIMIT`).

```sql
SELECT * FROM users ORDER BY id;
SELECT name FROM users ORDER BY name DESC;
```

```bash
cargo test -p rusql-executor select_order_by
cargo test -p rusql-server compat
```

---

## M16 — SELECT LIMIT (2026-06-30)

**What**: `SELECT * FROM tbl LIMIT n` caps result rows (with projection/WHERE).

```bash
cargo test -p rusql-executor select_limit
```

---

## M15 — USE database (2026-06-30)

**What**: `USE rusql` sets session default database; unknown DB names error.

```sql
USE rusql;
```

Note: `USE DATABASE name` is not parsed by our MySQL dialect yet; clients using `USE name` work.

```bash
cargo test -p rusql-executor use_database
cargo test -p rusql-server use_database
```

---

## M14 — SELECT column projection (2026-06-30)

**What**: `SELECT id, name FROM users` returns only listed columns; `SELECT *` unchanged.

**Try it**:

```sql
SELECT name FROM users;
SELECT id, name FROM users WHERE id = 1;
```

**Automated**:

```bash
cargo test -p rusql-executor select_column_projection
cargo test -p rusql-server run_basic_compat_fixtures
```

---

## Book — Harness Engineering narrative (#28)

**What**: mdBook in English and zh-CN — one chapter per milestone (M0–M13), Harness Engineering part, metrics appendix.

**Read**: [docs/book/README.md](../../docs/book/README.md)

**Build**:

```bash
cargo install mdbook   # once
node scripts/build-book.mjs
node scripts/check-book.mjs
```

---

## M13 — SHOW CREATE TABLE (2026-06-30)

**What**: `SHOW CREATE TABLE tbl` returns `Table` and `Create Table` columns with reconstructable DDL.

**Try it**:

```sql
CREATE TABLE users (id INT, name VARCHAR(32));
SHOW CREATE TABLE users;
```

**Automated**:

```bash
cargo test -p rusql-executor show_create
cargo test -p rusql-server run_basic_compat_fixtures
```

---

## M12 — DESCRIBE and information_schema (2026-06-30)

**What**: `DESCRIBE tbl`, `SHOW COLUMNS FROM tbl`, and virtual `information_schema.tables` / `information_schema.columns` for tooling.

**Try it**:

```sql
DESCRIBE users;
SHOW COLUMNS FROM users;
SELECT * FROM information_schema.tables;
SELECT * FROM information_schema.columns WHERE table_name = 'users';
```

**Automated**:

```bash
cargo test -p rusql-executor describe
cargo test -p rusql-executor information_schema
cargo test -p rusql-server describe
cargo test -p rusql-server run_basic_compat_fixtures
```

Spec: [m12-describe-info-schema.md](specs/m12-describe-info-schema.md)

---

## M11 — Prepared statements (2026-06-30)

**What**: `COM_STMT_PREPARE`, `COM_STMT_EXECUTE`, `COM_STMT_CLOSE`. Supports `?` placeholders (text/VARCHAR params).

**Try it** (via wire tests):

```bash
cargo test -p rusql-server stmt_prepare
```

**Automated**:

```bash
cargo test -p rusql-protocol stmt::
cargo test -p rusql-server stmt_
```

Spec: [m11-stmt-prepare.md](specs/m11-stmt-prepare.md)

---

## M10 — SHOW TABLES / SHOW DATABASES (2026-06-30)

**What**: List tables and the default `rusql` database (MySQL-style result columns).

**Try it**:

```sql
CREATE TABLE users (id INT);
SHOW TABLES;
SHOW DATABASES;
```

**Automated**:

```bash
cargo test -p rusql-executor show_tables
cargo test -p rusql-server compat
```

---

## M9 — Transactions (2026-06-30)

**What**: Explicit `BEGIN`, `COMMIT`, and `ROLLBACK`. Uncommitted writes are visible only to the same connection until commit.

**Try it**:

```bash
cargo run -p rusql-server -- --port 3307
```

```sql
CREATE TABLE t (id INT);
BEGIN;
INSERT INTO t VALUES (1);
SELECT * FROM t;
COMMIT;
```

**Automated**:

```bash
cargo test -p rusql-server transaction_commit_and_rollback
cargo test -p rusql-server compat
```

Spec: [m9-transactions.md](specs/m9-transactions.md)

---

## M8 — UPDATE

`UPDATE table SET col = value [WHERE col = value]` with WAL persistence.

```bash
cargo test -p rusql-server compat
```

---

## M7 — caching_sha2_password

Default auth plugin for MySQL 8 clients. Optional `--auth-password` enables verification.

```bash
cargo test -p rusql-protocol
```

Spec: [adr-m7-caching-sha2.md](specs/adr-m7-caching-sha2.md)

---

## M6 — Auth + DROP / DELETE

Password verification and destructive DML.

Spec: [adr-m6-auth-and-dml.md](specs/adr-m6-auth-and-dml.md)

---

## M5 — Compatibility fixtures

JSON-driven wire tests in `crates/rusql-server/compat/basic.json`.

```bash
cargo test -p rusql-server run_basic_compat_fixtures
```

---

## M4 — Indexes

`CREATE INDEX`, point lookup via `WHERE column = literal`.

---

## M3 — Persistence

`--data-dir` and WAL replay across restarts.

```bash
cargo test -p rusql-server persistence_across_connections
```

---

## M2 — COM_QUERY

SQL over MySQL wire protocol.

```bash
cargo test -p rusql-server com_query
```

---

## M1 — Handshake

MySQL v10 handshake.

```bash
cargo test -p rusql-protocol handshake
```

---

## M0 — Harness

Project bootstrap and CI sensors.

```bash
node scripts/harness-validate.mjs
node scripts/metrics.mjs
```

---

## Updating these notes

Every merged PR that changes user-visible behavior must:

1. Add a bullet under `CHANGELOG.md` → `[Unreleased]`
2. Add or update a **Latest** section here (move previous Latest down)
3. Update [user-guide.md](user-guide.md) test steps if needed

Sensor: `node scripts/check-changelog.mjs`
