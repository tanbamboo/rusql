## Goal

Parse, store, and execute stored procedures and stored functions (minimal subset).

## Category

Phase J — Stored programs.

## Depends on

- M46 built-in functions, M9 transactions

## Acceptance Criteria

- [ ] `CREATE PROCEDURE p() BEGIN … END` with simple DML body
- [ ] `CALL p()` from COM_QUERY
- [ ] `CREATE FUNCTION f() RETURNS …` scalar functions in SELECT
- [ ] `DROP PROCEDURE` / `DROP FUNCTION`
- [ ] `information_schema.ROUTINES` stub metadata
- [ ] Initial 5-case portable subset from `mysql-test/sp-*` donor files

## File Boundaries

- `crates/rusql-sql/**`, `crates/rusql-executor/**`, `crates/rusql-core/**`, `docs/en/specs/**`

## Negative Constraints

- No cursors, handlers, or dynamic SQL in v1
- No replication of stored program binlog events (M56)
