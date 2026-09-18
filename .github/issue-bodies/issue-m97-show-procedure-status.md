## Goal

Implement `SHOW PROCEDURE STATUS` so clients and GUIs that list stored procedures get MySQL-shaped columns instead of an unsupported-statement error.

## Background

Phase Q after M96. sqlparser 0.53 may not expose `Statement::ShowProcedureStatus`; `SHOW PROCEDURE STATUS` typically needs a parse rewrite (same pattern as `SHOW TRIGGERS` / `SHOW TABLE STATUS`). phpMyAdmin, mysqldump-style tools, and the official CLI send `SHOW PROCEDURE STATUS` (optional `LIKE`) after `CREATE PROCEDURE`. P3 already stores `ProcedureMeta` (schema, name, body statements) in `programs.json` and the session catalog. This slice lists catalog procedures with documented stub cells for Definer / timestamps / charset. `SHOW FUNCTION STATUS`, `SHOW CREATE EVENT` / `USER`, per-database charset clauses, and GTID event type 33 / heartbeat stay later.

## Acceptance Criteria

- [ ] `SHOW PROCEDURE STATUS` returns MySQL-shaped columns including at least `Db`, `Name`, `Type` (add `Definer`, `Modified`, `Created`, `Security_type`, `Comment`, `character_set_client`, `collation_connection`, `Database Collation` if a one-line MySQL-shaped row is natural)
- [ ] `Db` / `Name` come from catalog `ProcedureMeta`; `Type` is `PROCEDURE`; other cells are documented stubs
- [ ] `SHOW PROCEDURE STATUS LIKE 'pattern'` filters on procedure name; unmatched `LIKE` returns zero rows
- [ ] `SHOW CREATE FUNCTION` from M96, `SHOW CREATE PROCEDURE` from M95, and `SHOW CREATE TRIGGER` from M94 are unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` (Definer / timestamps / charset extras differ from Docker MySQL); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (rewrite/parse `SHOW PROCEDURE STATUS` if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- `SHOW ENGINE INNODB STATUS` / `SHOW ENGINE MUSQL STATUS`
- `SHOW FUNCTION STATUS`
- `SHOW CREATE EVENT` / `USER`
- Persisting `DEFINER` / `sql_mode` / timestamps on `CREATE PROCEDURE`
- Generating live truncation / strict-mode warnings
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 `SHOW PROCEDURE STATUS` parity; document stub Definer/timestamp/charset cells
- Do not change `SHOW CREATE FUNCTION` output from M96
- Do not implement `SHOW FUNCTION STATUS` or `SHOW CREATE EVENT`
- Do not implement `WHERE` filtering unless sqlparser already exposes it as a one-line wire-up (`LIKE` is required)

## Test plan

```bash
cargo test -p rusql-sql show_procedure_status
cargo test -p rusql-executor show_procedure_status
cargo test -p rusql-server show_procedure_status
```
