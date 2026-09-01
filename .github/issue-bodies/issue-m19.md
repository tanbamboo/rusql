## Goal

`LIMIT n OFFSET m` pagination.

## Depends on

- M16 SELECT LIMIT

## Acceptance Criteria

- [ ] `SELECT * FROM t LIMIT 2 OFFSET 1`
- [ ] Works with ORDER BY when M17 merged

## File Boundaries

- crates/rusql-executor/**
