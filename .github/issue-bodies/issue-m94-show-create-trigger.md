## Goal

Implement `SHOW CREATE TRIGGER` so clients and GUIs that probe trigger DDL get MySQL-shaped columns instead of an unsupported-statement error.

## Background

Phase Q after M93. sqlparser 0.53 may not expose `ShowCreateObject::Trigger`; `SHOW CREATE TRIGGER` typically needs a parse rewrite (same pattern as `SHOW CREATE DATABASE`). phpMyAdmin, mysqldump-style tools, and the official CLI send `SHOW CREATE TRIGGER` after `CREATE TRIGGER` / `SHOW TRIGGERS`. M48 already stores `TriggerMeta` (name, table, timing, event, body) in `programs.json` and the session catalog. This slice reconstructs documented `CREATE TRIGGER … FOR EACH ROW …` DDL from that catalog. Definer / sql_mode / charset cells stay stubs (not persisted on `CREATE TRIGGER`). `SHOW CREATE PROCEDURE` / `FUNCTION` / `EVENT` / `USER`, per-database charset clauses, and GTID event type 33 / heartbeat stay later.

## Acceptance Criteria

- [ ] `SHOW CREATE TRIGGER t` returns MySQL-shaped columns including at least `Trigger` and `SQL Original Statement` (add `sql_mode`, `character_set_client`, `collation_connection`, `Database Collation`, `Created` if a one-line MySQL-shaped row is natural)
- [ ] The `SQL Original Statement` cell is reconstructed from catalog `TriggerMeta` (`CREATE TRIGGER \`t\` {BEFORE|AFTER} {INSERT|UPDATE|DELETE} ON \`table\` FOR EACH ROW …` using stored timing/event/table/body); not DEFINER / sql_mode dump
- [ ] Unknown trigger is an error (same class as existing missing-trigger handling)
- [ ] `SHOW TRIGGERS` from M93, `SHOW CREATE VIEW` from M92, and `SHOW CREATE TABLE` from M13 are unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` (Definer / sql_mode / charset extras differ from Docker MySQL); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (rewrite/parse `SHOW CREATE TRIGGER` if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- `SHOW ENGINE INNODB STATUS` / `SHOW ENGINE MUSQL STATUS`
- `SHOW CREATE PROCEDURE` / `FUNCTION` / `EVENT` / `USER`
- Persisting `DEFINER` / `sql_mode` on `CREATE TRIGGER`
- Generating live truncation / strict-mode warnings
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 `SHOW CREATE TRIGGER` parity; document reconstructed catalog DDL (stub Definer/sql_mode/charset)
- Do not change `SHOW TRIGGERS` output from M93
- Do not implement `SHOW CREATE PROCEDURE` / `FUNCTION`
- Do not invent extra trigger timing/event types beyond M48 BEFORE/AFTER × INSERT/UPDATE/DELETE

## Test plan

```bash
cargo test -p rusql-sql show_create_trigger
cargo test -p rusql-executor show_create_trigger
cargo test -p rusql-server show_create_trigger
```
