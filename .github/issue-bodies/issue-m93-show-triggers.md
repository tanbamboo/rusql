## Goal

Implement `SHOW TRIGGERS` so clients and GUIs that list table triggers get MySQL-shaped columns instead of an unsupported-statement error.

## Background

Phase Q after M92. sqlparser 0.53 may not expose `Statement::ShowTriggers`; `SHOW TRIGGERS` typically needs a parse rewrite (same pattern as `SHOW TABLE STATUS` / `SHOW WARNINGS`). phpMyAdmin and the official CLI send `SHOW TRIGGERS` (optional `FROM`/`IN` db and `LIKE`) after `CREATE TRIGGER`. M48/P3 already persist trigger metadata in `programs.json` and seed the session catalog. This slice lists catalog triggers with documented stub cells for Definer / sql_mode / charset. `SHOW CREATE TRIGGER` / `PROCEDURE` / `FUNCTION`, per-database charset clauses, and GTID event type 33 / heartbeat stay later.

## Acceptance Criteria

- [ ] `SHOW TRIGGERS` returns MySQL-shaped columns including at least `Trigger`, `Event`, `Table`, `Statement`, `Timing` (add `Created`, `sql_mode`, `Definer`, `character_set_client`, `collation_connection`, `Database Collation` if a one-line MySQL-shaped row is natural)
- [ ] `Trigger` / `Event` / `Table` / `Timing` / `Statement` come from catalog `TriggerMeta` (not invented names); other cells are documented stubs
- [ ] `SHOW TRIGGERS LIKE 'pattern'` filters on trigger name; unmatched `LIKE` returns zero rows
- [ ] Optional `FROM`/`IN` db lists that database's triggers (unknown db: same class as existing missing-database handling)
- [ ] `SHOW CREATE VIEW` from M92, `SHOW CREATE TABLE` from M13, and `SHOW CREATE DATABASE` from M91 are unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` (Definer / sql_mode / charset extras differ from Docker MySQL); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (rewrite/parse `SHOW TRIGGERS` if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- `SHOW ENGINE INNODB STATUS` / `SHOW ENGINE MUSQL STATUS`
- `SHOW CREATE TRIGGER` / `PROCEDURE` / `FUNCTION` / `EVENT` / `USER`
- Persisting `DEFINER` / `sql_mode` on `CREATE TRIGGER`
- Generating live truncation / strict-mode warnings
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 `SHOW TRIGGERS` parity; document stub Definer/sql_mode/charset cells
- Do not change `SHOW CREATE VIEW` output from M92
- Do not implement `SHOW CREATE TRIGGER`
- Do not implement `WHERE` filtering unless sqlparser already exposes it as a one-line wire-up (`LIKE` is required)

## Test plan

```bash
cargo test -p rusql-sql show_triggers
cargo test -p rusql-executor show_triggers
cargo test -p rusql-server show_triggers
```
