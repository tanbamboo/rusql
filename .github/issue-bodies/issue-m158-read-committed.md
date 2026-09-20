## Goal

`SET TRANSACTION ISOLATION LEVEL READ COMMITTED` changes what a second statement in the same txn sees vs REPEATABLE READ snapshot.

## Category

Phase U — Transactions, locking, isolation (M158). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M84 SET TRANSACTION exists as overlay. M32 MVCC snapshot. This slice: RC sees committed writes from others between statements; RR does not (current snapshot).


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] RR: A BEGIN; A SELECT; B COMMIT insert; A SELECT again — A does not see B’s row
- [ ] RC: same sequence — A’s second SELECT sees B’s committed row
- [ ] Default stays documented (RR-like snapshot today). SET TRANSACTION from M84 is the switch
- [ ] Tests with two connections; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-storage/src/txn.rs`
- `crates/rusql-storage/src/**` (MVCC)
- `crates/rusql-executor/src/session_var.rs`
- `crates/rusql-server/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- SERIALIZABLE (M159)
- Gap locks (M161)
- Breaking snapshot for existing RR tests

## Negative Constraints

- Do not implement READ UNCOMMITTED dirty reads
- Do not change autocommit

## Test plan

```bash
cargo test -p rusql-storage read_committed
cargo test -p rusql-server read_committed
```
