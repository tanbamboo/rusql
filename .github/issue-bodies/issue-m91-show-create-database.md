## Goal

Implement `SHOW CREATE DATABASE` / `SHOW CREATE SCHEMA` so clients and GUIs that probe schema DDL get MySQL-shaped columns instead of an unsupported-statement error.

## Background

Phase Q after M90. sqlparser 0.53 may not expose `ShowCreateObject::Database`; `SHOW CREATE DATABASE` typically needs a parse rewrite (same pattern as `SHOW WARNINGS` / `SHOW ENGINES`). phpMyAdmin, mysqldump-style tools, and the official CLI send `SHOW CREATE DATABASE` after `CREATE DATABASE` / `SHOW DATABASES`. This slice is a documented stub DDL aligned with rusql's utf8mb4 default collation (M59/M87/M89). Per-database charset clauses, `SHOW CREATE PROCEDURE` / `FUNCTION` / `TRIGGER`, and GTID event type 33 / heartbeat stay later.

## Acceptance Criteria

- [ ] `SHOW CREATE DATABASE db` returns MySQL-shaped columns `Database`, `Create Database`
- [ ] The `Create Database` cell is documented stub DDL for an existing database (include `utf8mb4` and rusql's documented default collation `utf8mb4_unicode_ci`); values are constants, not a live per-schema charset catalog
- [ ] `SHOW CREATE SCHEMA db` is equivalent to `SHOW CREATE DATABASE db` for this slice
- [ ] Unknown database is an error (same class as existing missing-database handling; MySQL errno 1049 if a one-line existing path already maps it)
- [ ] `SHOW CREATE TABLE` from M13 and `SHOW WARNINGS` stubs from M90 are unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` (stub charset/collation comments differ from Docker MySQL); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (rewrite/parse `SHOW CREATE DATABASE` / `SHOW CREATE SCHEMA` if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- `SHOW ENGINE INNODB STATUS` / `SHOW ENGINE MUSQL STATUS`
- `SHOW CREATE PROCEDURE` / `FUNCTION` / `TRIGGER` / `VIEW` / `EVENT` / `USER`
- Persisting per-database `CHARACTER SET` / `COLLATE` on `CREATE DATABASE`
- Generating live truncation / strict-mode warnings
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 `SHOW CREATE DATABASE` parity; document the stub DDL
- Do not change `SHOW CREATE TABLE` output from M13
- Do not implement `CREATE DATABASE … CHARACTER SET` / `COLLATE` clauses
- Do not implement `IF NOT EXISTS` variations of SHOW CREATE

## Test plan

```bash
cargo test -p rusql-sql show_create_database
cargo test -p rusql-executor show_create_database
cargo test -p rusql-server show_create_database
```
