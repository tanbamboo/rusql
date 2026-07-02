# M31 — Durable COMMIT WAL

**Issue #54**

## Problem

M9 added per-connection transaction overlays with deferred writes, but operators need **committed** work to survive `mysqld` restart — the same bar as autocommit DML from M3.

## Decision

- `COMMIT` calls `PersistentEngine::commit_transaction`, appending each pending `WalRecord` with `sync_data` and applying to the shared heap.
- `ROLLBACK` drops the overlay without touching the WAL file.
- Storage tests prove commit survives `PersistentEngine::open` replay; wire tests prove `COMMIT`/`ROLLBACK` WAL semantics and post-commit reopen.

## Harness lesson

> When behavior already exists from an earlier milestone, ship the **next** milestone as explicit acceptance tests + docs so regressions are visible in CI.
