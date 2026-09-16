## Goal

Implement `SET CHARACTER SET` as a `SET NAMES` alias and `SELECT @foo := expr` assignment expressions so clients that mix charset aliases and inline user-var assignment get M82 persistence instead of parse/execute errors.

## Background

Phase Q after M82. `SET NAMES` and `SET @foo = expr` persist per connection. Some connectors send `SET CHARACTER SET utf8mb4` / `SET CHARSET utf8mb4`, and application SQL uses `SELECT @foo := 1`. Wire charset conversion remains later.

## Acceptance Criteria

- [ ] `SET CHARACTER SET utf8mb4` and `SET CHARSET utf8mb4` overlay the same charset stubs as `SET NAMES utf8mb4` on that connection
- [ ] `SELECT @foo := 1` assigns `1` on that connection and returns `1`; subsequent `SELECT @foo` sees it
- [ ] Unset user variables remain empty/NULL as in M82
- [ ] Assignments are in-memory per session (not WAL). `COM_RESET_CONNECTION` / `COM_CHANGE_USER` still clear user vars and restore charset stubs
- [ ] `SET NAMES` / `SET @foo = expr` behavior from M82 is unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` where Docker MySQL charset/CLI encoding differs; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (parse `SET CHARACTER SET` / `SET CHARSET` / `@foo :=` if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-core/src/**` (only if assignment needs extra session fields)
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- Wire charset conversion / recoding result bytes
- `SET TRANSACTION ISOLATION LEVEL`
- Persisting variables across server restart
- New crates

## Negative Constraints

- Do not implement MySQL charset conversion of result bytes; overlays only
- Do not implement `SET PASSWORD` / `SET ROLE` / other SET forms
- Do not change autocommit engine behavior

## Test plan

```bash
cargo test -p rusql-executor set_charset
cargo test -p rusql-executor user_var
cargo test -p rusql-server set_charset
cargo test -p rusql-server user_var
```
