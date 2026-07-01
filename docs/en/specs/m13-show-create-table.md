# M13: SHOW CREATE TABLE

## Goal

MySQL-compatible DDL export for schema backup and ORM introspection.

## Acceptance criteria

- [x] `SHOW CREATE TABLE tbl` returns `Table` and `Create Table` columns
- [x] DDL reflects catalog column types (INT, VARCHAR(n), etc.)
- [x] Wire/executor tests + CHANGELOG + release notes

## Boundaries

- Tables only (no views, triggers, procedures)
- No `IF NOT EXISTS` or engine/charset clauses

## Decisions

| Topic | Choice |
|-------|--------|
| Quoting | Backtick table and column names |
| Types | Uppercase from catalog metadata |
