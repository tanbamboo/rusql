## Goal

Implement `SHOW FUNCTION STATUS` so clients and GUIs that list stored functions get MySQL-shaped columns instead of an unsupported-statement error.

## Background

Phase Q after M97. sqlparser 0.53 may not expose `Statement::ShowFunctionStatus`; `SHOW FUNCTION STATUS` typically needs a parse rewrite (same pattern as `SHOW PROCEDURE STATUS`). phpMyAdmin, mysqldump-style tools, and the official CLI send `SHOW FUNCTION STATUS` (optional `LIKE`) after `CREATE FUNCTION`. M63 already stores `FunctionMeta` (schema, name, return_type, return_expr) in `programs.json` and the session catalog. This slice lists catalog functions with documented stub cells for Definer / timestamps / charset. `SHOW CREATE EVENT` / `USER`, per-database charset clauses, and GTID event type 33 / heartbeat stay later.

## Acceptance Criteria

- [ ] `SHOW FUNCTION STATUS` returns MySQL-shaped columns including at least `Db`, `Name`, `Type` (add `Definer`, `Modified`, `Created`, `Security_type`, `Comment`, `character_set_client`, `collation_connection`, `Database Collation` if a one-line MySQL-shaped row is natural)
- [ ] `Db` / `Name` come from catalog `FunctionMeta`; `Type` is `FUNCTION`; other cells are documented stubs
- [ ] `SHOW FUNCTION STATUS LIKE 'pattern'` filters on function name; unmatched `LIKE` returns zero rows
- [ ] `SHOW PROCEDURE STATUS` from M97, `SHOW CREATE FUNCTION` from M96, and `SHOW CREATE PROCEDURE` from M95 are unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` (Definer / timestamps / charset extras differ from Docker MySQL); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (rewrite/parse `SHOW FUNCTION STATUS` if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- `SHOW ENGINE INNODB STATUS` / `SHOW ENGINE MUSQL STATUS`
- `SHOW CREATE EVENT` / `USER`
- Persisting `DEFINER` / `sql_mode` / timestamps on `CREATE FUNCTION`
- Generating live truncation / strict-mode warnings
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 `SHOW FUNCTION STATUS` parity; document stub Definer/timestamp/charset cells
- Do not change `SHOW PROCEDURE STATUS` output from M97
- Do not implement `SHOW CREATE EVENT`
- Do not implement `WHERE` filtering unless sqlparser already exposes it as a one-line wire-up (`LIKE` is required)

## Test plan

```bash
cargo test -p rusql-sql show_function_status
cargo test -p rusql-executor show_function_status
cargo test -p rusql-server show_function_status
```
