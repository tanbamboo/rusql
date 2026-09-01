## Goal

`COMMIT` writes transaction overlay to WAL; restart sees committed data.

## Depends on

- M3 WAL, M9 transaction overlay

## Acceptance Criteria

- [ ] COMMIT durable across restart
- [ ] ROLLBACK still discards without WAL flush

## File Boundaries

- crates/rusql-storage/**, crates/rusql-server/**
