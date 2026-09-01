## Goal

Support stored functions callable from SELECT (completes M47 beyond procedure MVP).

## Category

Phase J — Stored programs (post-M61).

## Depends on

- M47 procedure MVP

## Acceptance Criteria

- [ ] `CREATE FUNCTION f() RETURNS INT …` parsed and persisted
- [ ] `SELECT f()` and `SELECT f() + 1` evaluate scalar return
- [ ] `DROP FUNCTION`
- [ ] `information_schema.ROUTINES` shows `ROUTINE_TYPE = FUNCTION`
- [ ] Unit tests for create/call/drop

## File Boundaries

- `crates/rusql-sql/**`, `crates/rusql-executor/**`, `crates/rusql-core/**`

## Negative Constraints

- No cursors, handlers, or dynamic SQL
- Single-statement function bodies only in v1
