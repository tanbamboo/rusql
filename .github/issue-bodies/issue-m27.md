## Goal

`information_schema.SCHEMATA`, `STATISTICS` minimal columns.

## Depends on

- M12 information_schema subset

## Acceptance Criteria

- [ ] `SELECT * FROM information_schema.SCHEMATA`
- [ ] Index stats for SHOW INDEX prep

## File Boundaries

- crates/rusql-executor/**
