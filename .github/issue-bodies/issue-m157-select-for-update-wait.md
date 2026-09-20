## Goal

Make `SELECT … FOR UPDATE` wait (or time out) on same-row writers instead of a no-op (M85).

## Category

Phase U — Transactions, locking, isolation (M157). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M85 is probe/no-op. Production ORM locking needs a second connection to block on UPDATE/SELECT FOR UPDATE of the same PK until commit/rollback or innodb_lock_wait_timeout.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] Connection A: BEGIN; SELECT … FOR UPDATE on id=1. Connection B: UPDATE same row — B blocks until A COMMIT/ROLLBACK or timeout
- [ ] Timeout errno 1205 (i18n). After A COMMIT, B proceeds
- [ ] M85 syntax still accepted. Isolation overlay documented until M158
- [ ] Concurrent tests; docs as usual; mysql-diff may stay single-connection — add a server lock test

## File Boundaries

Allowed:
- `crates/rusql-storage/src/txn.rs`
- `crates/rusql-storage/src/**` (row lock table)
- `crates/rusql-server/src/connection.rs`
- `crates/rusql-executor/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Gap / next-key locks (M161)
- XA (M162)
- Changing WAL record types except additive lock metadata if required

## Negative Constraints

- Do not deadlock without detection (M160) — two-connection same-row is wait, not cycle
- Do not implement SKIP LOCKED / NOWAIT unless tests pin errno 3572/1205

## Test plan

```bash
cargo test -p rusql-storage for_update_wait
cargo test -p rusql-server for_update_wait
```
