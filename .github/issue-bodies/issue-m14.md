## Goal

`SELECT col1, col2 FROM tbl` column projection (not only `SELECT *`).

## Acceptance Criteria

- [ ] `SELECT id FROM users` returns one column
- [ ] `SELECT name, id FROM users` column order matches projection
- [ ] `SELECT id FROM users WHERE id = 1` works with index path
- [ ] `SELECT * FROM tbl` unchanged
- [ ] Executor + compat tests + CHANGELOG + release notes + book chapter

## File Boundaries

- crates/rusql-executor/**
- crates/rusql-server/compat/basic.json
- docs/**
