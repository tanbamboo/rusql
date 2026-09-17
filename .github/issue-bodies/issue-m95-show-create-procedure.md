## Goal

Implement `SHOW CREATE PROCEDURE` so clients and GUIs that probe stored-procedure DDL get MySQL-shaped columns instead of an unsupported-statement error.

## Background

Phase Q after M94. sqlparser 0.53 may not expose `ShowCreateObject::Procedure`; `SHOW CREATE PROCEDURE` typically needs a parse rewrite (same pattern as `SHOW CREATE TRIGGER` / `SHOW CREATE DATABASE`). phpMyAdmin, mysqldump-style tools, and the official CLI send `SHOW CREATE PROCEDURE` after `CREATE PROCEDURE`. P3 already stores `ProcedureMeta` (schema, name, body statements) in `programs.json` and the session catalog. This slice reconstructs documented `CREATE PROCEDURE … BEGIN … END` DDL from that catalog. Parameter lists are not persisted on `CREATE PROCEDURE` today — reconstruct `()` (empty params) rather than inventing signatures. Definer / sql_mode / charset cells stay stubs. `SHOW CREATE FUNCTION` / `EVENT` / `USER`, `SHOW PROCEDURE STATUS`, per-database charset clauses, and GTID event type 33 / heartbeat stay later.

## Acceptance Criteria

- [ ] `SHOW CREATE PROCEDURE p` returns MySQL-shaped columns including at least `Procedure` and `Create Procedure` (add `sql_mode`, `character_set_client`, `collation_connection`, `Database Collation` if a one-line MySQL-shaped row is natural)
- [ ] The `Create Procedure` cell is reconstructed from catalog `ProcedureMeta` (`CREATE PROCEDURE \`p\`() BEGIN … END` using stored body statements and an empty parameter list); not DEFINER / sql_mode dump and not invented IN/OUT params
- [ ] Unknown procedure is an error (same class as existing missing-procedure handling; MySQL-like errno 1305 if a one-line `ExecError::Mysql` path is natural)
- [ ] `SHOW CREATE TRIGGER` from M94, `SHOW TRIGGERS` from M93, and `SHOW CREATE VIEW` from M92 are unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` (Definer / sql_mode / charset extras differ from Docker MySQL); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (rewrite/parse `SHOW CREATE PROCEDURE` if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- `SHOW ENGINE INNODB STATUS` / `SHOW ENGINE MUSQL STATUS`
- `SHOW CREATE FUNCTION` / `EVENT` / `USER`
- Persisting `DEFINER` / `sql_mode` / parameter signatures on `CREATE PROCEDURE`
- Generating live truncation / strict-mode warnings
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 `SHOW CREATE PROCEDURE` parity; document reconstructed catalog DDL (empty param list, stub Definer/sql_mode/charset)
- Do not change `SHOW CREATE TRIGGER` output from M94
- Do not implement `SHOW CREATE FUNCTION` or `SHOW PROCEDURE STATUS`
- Do not invent stored-procedure parameters, characteristics, or SECURITY clauses

## Test plan

```bash
cargo test -p rusql-sql show_create_procedure
cargo test -p rusql-executor show_create_procedure
cargo test -p rusql-server show_create_procedure
```
