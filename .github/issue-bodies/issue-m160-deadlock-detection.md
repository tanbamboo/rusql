## Goal

Detect lock cycles and abort one waiter with errno 1213.

## Category

Phase U — Transactions, locking, isolation (M160). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

After M157 waits. Two connections updating rows in opposite order must not hang the server. Victim selection can be simple (younger txn).


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] A: lock row 1 then wait row 2; B: lock row 2 then wait row 1 → one connection gets errno 1213 (i18n), the other proceeds
- [ ] No process hang > lock wait timeout. SHOW ENGINE / performance_schema not required
- [ ] Unit/integration tests; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-storage/src/txn.rs`
- `crates/rusql-storage/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Gap locks (M161)
- Changing binlog on deadlock beyond existing rollback

## Negative Constraints

- Do not implement deadlock weight heuristics beyond a documented rule
- Do not kill connections (rollback statement/txn only)

## Test plan

```bash
cargo test -p rusql-storage deadlock
cargo test -p rusql-server deadlock
```
