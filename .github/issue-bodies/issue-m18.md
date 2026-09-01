## Goal

`SELECT col AS alias` output column names.

## Depends on

- M14 SELECT column projection

## Acceptance Criteria

- [ ] `SELECT id AS user_id FROM t` returns column `user_id`
- [ ] Tests + compat + docs

## File Boundaries

- crates/rusql-executor/**
