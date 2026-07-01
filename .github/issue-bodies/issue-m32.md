## Goal

Snapshot isolation with multi-version rows (replace single-writer overlay).

## Depends on

- M31 durable COMMIT

## Acceptance Criteria

- [ ] Two connections: reader sees consistent snapshot
- [ ] Writer does not block reader (basic MVCC)

## File Boundaries

- crates/rusql-storage/**
