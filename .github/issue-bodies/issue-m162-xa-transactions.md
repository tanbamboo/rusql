## Goal

Two-phase XA: `XA START`/`END`/`PREPARE`/`COMMIT`/`ROLLBACK` with persisted prepare.

## Category

Phase U — Transactions, locking, isolation (M162). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Needs-human (durability semantics). Persist prepared XA in WAL/catalog. Recover PREPARED on restart. Keep slice to one-branch xid.


**Sequencing**: not `agent-ready`. Phase X / ADR-gated work stays `needs-human` until an ADR is accepted. Do not start while M114 file boundaries overlap.

## Acceptance Criteria

- [ ] `XA START 'xid'`; DML; `XA END`; `XA PREPARE`; `XA COMMIT` makes rows durable; crash after PREPARE still lists xid in `XA RECOVER`
- [ ] `XA ROLLBACK` after PREPARE drops the txn. Invalid state transitions error (errno 1399 / documented)
- [ ] Ordinary BEGIN/COMMIT unchanged
- [ ] Tests including restart; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-storage/src/**`
- `crates/rusql-protocol/src/**` (if OK packet flags needed)
- `crates/rusql-server/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- XA over replication (group)
- Breaking existing WAL records (additive prepare records only)
- New crates

## Negative Constraints

- Do not implement XA JOIN/SUSPEND
- Do not add TMJOIN one-phase optimizations

## Test plan

```bash
cargo test -p rusql-storage xa
cargo test -p rusql-server xa
```
