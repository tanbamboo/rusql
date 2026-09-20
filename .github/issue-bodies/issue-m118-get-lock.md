## Goal

Implement advisory `GET_LOCK(name, timeout)` and `RELEASE_LOCK(name)` so app-level locks are not `unsupported function`.

## Background

Phase R. Probe: `fn_get_lock` with timeout `0` (non-blocking). Connection-scoped named locks; released on `COM_RESET_CONNECTION` / disconnect. Waiting (`timeout>0`) may be a later slice (M164) if this issue only does timeout 0.

## Acceptance Criteria

- [ ] `SELECT GET_LOCK('gap_lock', 0)` returns `1` when the name is free
- [ ] A second connection `GET_LOCK('gap_lock', 0)` returns `0` while the first holds it
- [ ] `SELECT RELEASE_LOCK('gap_lock')` on the holder returns `1` and frees the name; on a non-holder returns `0` or NULL matching MySQL (pin tests)
- [ ] Disconnect / `COM_RESET_CONNECTION` releases locks held by that session
- [ ] Unit/wire tests; `mysql-diff` where deterministic; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF, rusql-vs-mysql

## File Boundaries

Allowed:
- `crates/rusql-core/src/**` (lock registry)
- `crates/rusql-executor/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- InnoDB row locks / `FOR UPDATE` wait (M157)
- WAL format
- New crates

## Negative Constraints

- Do not implement `IS_FREE_LOCK` / `IS_USED_LOCK` unless one-line aliases with tests
- Timeout > 0 may return immediately as 0 (document) **or** wait; if wait, add a test that does not hang CI

## Test plan

```bash
cargo test -p rusql-executor get_lock
cargo test -p rusql-server get_lock
```
