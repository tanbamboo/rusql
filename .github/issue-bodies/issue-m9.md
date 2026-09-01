## Goal

Implement per-connection `BEGIN` / `COMMIT` / `ROLLBACK` with deferred WAL commit and connection-local overlay isolation.

## Acceptance Criteria

- [ ] `BEGIN` / `START TRANSACTION` returns OK
- [ ] Uncommitted writes invisible to other connections
- [ ] `COMMIT` persists to WAL; `ROLLBACK` discards
- [ ] Compat `transactions` suite + wire integration test
- [ ] Spec: docs/en/specs/m9-transactions.md

## File Boundaries

- crates/rusql-storage/**
- crates/rusql-server/**
- docs/**
- scripts/**
- profiles/rust/sensors.yaml

## Negative Constraints

- No savepoints, no COM_STMT_*

## Harness (same PR)

- scripts/metrics.mjs, doc-parity.mjs, check-handoff.mjs, mysql-diff.mjs
- adr-replication.md closes #5
