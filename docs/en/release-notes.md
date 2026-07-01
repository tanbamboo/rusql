# Release Notes

What landed on `main` and how to verify it. For day-to-day usage see [user-guide.md](user-guide.md).

**中文**: [release-notes.md](../zh-CN/release-notes.md)

---

## Latest: M10 — SHOW TABLES / SHOW DATABASES (2026-06-30)

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
