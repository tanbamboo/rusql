## Goal

If M118 was non-blocking, make `GET_LOCK(name, timeout)` wait up to timeout seconds.

## Category

Phase U — Transactions, locking, isolation (M164). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M118 advisory locks. This slice is timeout>0 blocking wait and `IS_USED_LOCK`/`IS_FREE_LOCK` if missing. Depends on M118 on main.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `GET_LOCK('a', 2)` on a held lock returns 0 after ~2s (or 1 if released in time). timeout 0 stays non-blocking
- [ ] `RELEASE_LOCK` from holder unblocks waiter. Connection close releases locks
- [ ] M118 names still session-scoped. Docs as usual

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-core/src/**` (session lock map)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Table row locks (M157)
- Changing GET_LOCK return type
- WAL

## Negative Constraints

- Do not implement `GET_LOCK` reentrancy beyond MySQL (same session returns 1)
- Do not add named mutexes outside SQL functions

## Test plan

```bash
cargo test -p rusql-executor get_lock_timeout
cargo test -p rusql-server get_lock_timeout
```
