## Goal

Implement `SHOW CREATE FUNCTION` so clients and GUIs that probe stored-function DDL get MySQL-shaped columns instead of an unsupported-statement error.

## Background

Phase Q after M95. sqlparser 0.53 may not expose `ShowCreateObject::Function`; `SHOW CREATE FUNCTION` typically needs a parse rewrite (same pattern as `SHOW CREATE PROCEDURE`). phpMyAdmin, mysqldump-style tools, and the official CLI send `SHOW CREATE FUNCTION` after `CREATE FUNCTION`. M63 already stores `FunctionMeta` (schema, name, return_type, return_expr) in `programs.json` and the session catalog. This slice reconstructs documented `CREATE FUNCTION …() RETURNS … BEGIN RETURN … END` DDL from that catalog. Parameter lists are not persisted on `CREATE FUNCTION` today — reconstruct `()` (empty params) rather than inventing signatures. Definer / sql_mode / charset / DETERMINISTIC cells stay stubs. `SHOW CREATE EVENT` / `USER`, `SHOW FUNCTION STATUS`, per-database charset clauses, and GTID event type 33 / heartbeat stay later.

## Acceptance Criteria

- [ ] `SHOW CREATE FUNCTION f` returns MySQL-shaped columns including at least `Function` and `Create Function` (add `sql_mode`, `character_set_client`, `collation_connection`, `Database Collation` if a one-line MySQL-shaped row is natural)
- [ ] The `Create Function` cell is reconstructed from catalog `FunctionMeta` (`CREATE FUNCTION \`f\`() RETURNS {return_type} BEGIN RETURN {return_expr}; END` with an empty parameter list); not DEFINER / sql_mode dump and not invented IN/OUT params
- [ ] Unknown function is an error (same class as existing missing-function handling; MySQL-like errno 1305 if a one-line `ExecError::Mysql` path is natural)
- [ ] `SHOW CREATE PROCEDURE` from M95, `SHOW CREATE TRIGGER` from M94, and `SHOW CREATE VIEW` from M92 are unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` (Definer / sql_mode / charset extras differ from Docker MySQL); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (rewrite/parse `SHOW CREATE FUNCTION` if needed)
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
- Persisting `DEFINER` / `sql_mode` / parameter signatures on `CREATE FUNCTION`
- Generating live truncation / strict-mode warnings
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 `SHOW CREATE FUNCTION` parity; document reconstructed catalog DDL (empty param list, stub Definer/sql_mode/charset)
- Do not change `SHOW CREATE PROCEDURE` output from M95
- Do not implement `SHOW CREATE EVENT` or `SHOW FUNCTION STATUS`
- Do not invent stored-function parameters, characteristics, or SECURITY clauses

## Test plan

```bash
cargo test -p rusql-sql show_create_function
cargo test -p rusql-executor show_create_function
cargo test -p rusql-server show_create_function
```
