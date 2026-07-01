# M12: DESCRIBE and information_schema subset

## Goal

MySQL-compatible table introspection for ORMs, GUI tools, and drivers.

## Acceptance criteria

- [x] `DESCRIBE tbl` / `DESC tbl` returns Field/Type/Null/Key/Default/Extra
- [x] `SHOW COLUMNS FROM tbl` same shape
- [x] `SELECT * FROM information_schema.tables` lists catalog tables
- [x] `SELECT * FROM information_schema.columns WHERE table_name = 'x'`
- [x] Wire/executor tests + CHANGELOG + release notes

## Boundaries

- Virtual `information_schema` only (`tables`, `columns`); no persistence
- `EXPLAIN` query plans not supported (use `DESCRIBE` / `DESC`)
- Minimal column sets; no views, routines, or grants

## Decisions

| Topic | Choice |
|-------|--------|
| Schema name | Fixed `rusql` (`DEFAULT_SCHEMA`) |
| DESCRIBE types | Lowercase from catalog metadata |
| Column filter | `WHERE table_name = literal` only on `columns` |
