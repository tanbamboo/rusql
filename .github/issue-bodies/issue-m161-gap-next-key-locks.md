## Goal

ADR plus phantom prevention on a tested range under REPEATABLE READ (InnoDB next-key).

## Category

Phase U — Transactions, locking, isolation (M161). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Trust: new lock manager is L1-stop without ADR. needs-human until ADR accepted. Test: RR + INSERT into a locked range by another txn waits or errors like MySQL.


**Sequencing**: not `agent-ready`. Phase X / ADR-gated work stays `needs-human` until an ADR is accepted. Do not start while M114 file boundaries overlap.

## Acceptance Criteria

- [ ] ADR describing gap vs record vs next-key and what rusql will implement
- [ ] After ADR: a FOR UPDATE range scan prevents a concurrent INSERT into that gap (wait/timeout) on the fixture
- [ ] No merge of lock-manager code before ADR review
- [ ] Docs as usual

## File Boundaries

Allowed:
- `docs/en/architecture/**`, `docs/zh-CN/architecture/**`
- `crates/rusql-storage/src/**` (after ADR)
- `crates/rusql-server/src/**`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Shipping gap locks without ADR
- XA in the same PR
- New crates

## Negative Constraints

- Do not claim full InnoDB lock compatibility matrix
- Do not implement predicate locks for SERIALIZABLE as a silent extra

## Test plan

```bash
cargo test -p rusql-storage gap_lock
cargo test -p rusql-server gap_lock
```
