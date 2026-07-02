# Release Notes

What landed on `main` and how to verify it. For day-to-day usage see [user-guide.md](user-guide.md).

**中文**: [release-notes.md](../zh-CN/release-notes.md)

---

## Latest: M26 — caching_sha2 RSA full auth (2026-06-30)

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
