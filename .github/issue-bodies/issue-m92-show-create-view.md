## Goal

Implement `SHOW CREATE VIEW` so clients and GUIs that probe view DDL get MySQL-shaped columns instead of an unsupported-statement error.

## Background

Phase Q after M91. sqlparser 0.53 already exposes `Statement::ShowCreate` with `ShowCreateObject::Table`; `SHOW CREATE VIEW` typically lands as `ShowCreateObject::View` (wire up if present) or needs a parse rewrite like M91. phpMyAdmin, mysqldump-style tools, and the official CLI send `SHOW CREATE VIEW` after `CREATE VIEW` / `information_schema.VIEWS`. M33 already stores the view SELECT in the catalog (`ViewMeta.sql`). This slice reconstructs documented `CREATE VIEW … AS …` DDL from that catalog (not ALGORITHM / DEFINER / SQL SECURITY). `SHOW CREATE PROCEDURE` / `FUNCTION` / `TRIGGER`, per-database charset clauses, and GTID event type 33 / heartbeat stay later.

## Acceptance Criteria

- [ ] `SHOW CREATE VIEW v` returns MySQL-shaped columns `View`, `Create View` (and documented stub `character_set_client` / `collation_connection` if a one-line MySQL 4-column path is natural)
- [ ] The `Create View` cell is reconstructed from the catalog SELECT for an existing view (`CREATE VIEW \`v\` AS …` using stored `ViewMeta.sql`); not ALGORITHM / DEFINER / SQL SECURITY
- [ ] Unknown view is an error (same class as existing missing-table handling; MySQL errno 1146 if a one-line existing path already maps it)
- [ ] `SHOW CREATE TABLE` from M13, `SHOW CREATE DATABASE` from M91, and `SHOW WARNINGS` from M90 are unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` (DEFINER / charset extras differ from Docker MySQL); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (rewrite/parse `SHOW CREATE VIEW` if sqlparser does not expose `ShowCreateObject::View`)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- `SHOW ENGINE INNODB STATUS` / `SHOW ENGINE MUSQL STATUS`
- `SHOW CREATE PROCEDURE` / `FUNCTION` / `TRIGGER` / `EVENT` / `USER`
- Persisting `ALGORITHM` / `DEFINER` / `SQL SECURITY` on `CREATE VIEW`
- Persisting per-database `CHARACTER SET` / `COLLATE` on `CREATE DATABASE`
- Generating live truncation / strict-mode warnings
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 `SHOW CREATE VIEW` parity; document reconstructed catalog DDL (no DEFINER dump)
- Do not change `SHOW CREATE TABLE` output from M13 or `SHOW CREATE DATABASE` stubs from M91
- Do not implement `CREATE OR REPLACE VIEW` or materialized views
- Do not implement `IF NOT EXISTS` variations of SHOW CREATE

## Test plan

```bash
cargo test -p rusql-sql show_create_view
cargo test -p rusql-executor show_create_view
cargo test -p rusql-server show_create_view
```
