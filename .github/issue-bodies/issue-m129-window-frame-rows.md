## Goal

Honor window `ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW` on `ROW_NUMBER`/`RANK`/`DENSE_RANK` so the probe query is not a parse/exec error.

## Background

M70 ranking windows exist without frames. Probe: `window_frame_rows`. For ranking functions, this frame is a no-op vs default; still must parse and return the same ranks.

## Acceptance Criteria

- [ ] Probe SQL with `OVER (ORDER BY id ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW)` returns ranks matching unframed `ROW_NUMBER()`
- [ ] Unsupported frames (`RANGE BETWEEN`, named windows) still error clearly
- [ ] M70 without frames unchanged
- [ ] Unit/wire tests; `mysql-diff`; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF, rusql-vs-mysql

## File Boundaries

Allowed:
- `crates/rusql-executor/src/window.rs` and related executor
- `crates/rusql-sql/src/**` if rewrite required
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Aggregate windows (`SUM() OVER`) — M135
- WAL format

## Negative Constraints

- Do not claim full frame peer groups for aggregates in this issue

## Test plan

```bash
cargo test -p rusql-executor window_frame
cargo test -p rusql-server window_frame
```
