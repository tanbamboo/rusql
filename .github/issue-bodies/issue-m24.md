## Goal

`ALTER TABLE t ADD COLUMN c INT`.

## Depends on

- M23 PK metadata (catalog evolution)

## Acceptance Criteria

- [ ] ADD COLUMN updates catalog + storage
- [ ] WAL record for replay
- [ ] Tests

## File Boundaries

- crates/rusql-executor/**, crates/rusql-storage/**
