## Goal

`SHOW CREATE TABLE` for schema export and ORM introspection (MySQL-compatible DDL string).

## Acceptance Criteria

- [ ] `SHOW CREATE TABLE tbl` returns `Table` and `Create Table` columns
- [ ] DDL reflects catalog column types (INT, VARCHAR(n), etc.)
- [ ] Wire/executor tests + CHANGELOG + release notes

## File Boundaries

- crates/rusql-executor/**
- crates/rusql-server/compat/basic.json
- docs/**
