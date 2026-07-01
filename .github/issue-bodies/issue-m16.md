## Goal

`SELECT … FROM tbl LIMIT n` row cap for pagination smoke tests.

## Acceptance Criteria

- [ ] `LIMIT` literal integer on table SELECT
- [ ] Works with projection and WHERE
- [ ] Compat + executor tests + docs

## File Boundaries

- crates/rusql-executor/**
- crates/rusql-server/compat/basic.json
- docs/**
