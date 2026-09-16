## Goal

Implement `SET @@var` / `SET SESSION var` persistence for the documented M77+M79 stub catalog so a connection that assigns a stub (especially `autocommit`) sees the new value from `SELECT @@var` and `SHOW VARIABLES` on that same session.

## Background

Phase Q after M80. `SELECT @@` and `SHOW VARIABLES` still return compile-time stubs. JDBC/ORMs often send `SET autocommit=0` or `SET @@session.autocommit=0` after connect; ignoring SET leaves clients believing the change applied. Full MySQL sysvar semantics, `SET NAMES`, and user variables `@foo` remain later.

## Acceptance Criteria

- [ ] `SET @@autocommit = 0` then `SELECT @@autocommit` / `SELECT @@session.autocommit` return `0` on that connection
- [ ] `SET SESSION autocommit = 1` (and `SET @@session.autocommit = 1`) restore `1` on that connection
- [ ] `SHOW VARIABLES` / `SHOW SESSION VARIABLES` reflect the overlay for that name; other connections still see documented defaults
- [ ] Assignments are in-memory per session (not WAL). `COM_RESET_CONNECTION` / `COM_CHANGE_USER` restore documented stub defaults
- [ ] Unknown names still return errno 1193 via existing i18n
- [ ] `SET GLOBAL …` is rejected (session-only this slice; document the error)
- [ ] `SET NAMES` and user variables `@foo` remain unimplemented
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` where Docker MySQL defaults or writability differ; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (parse `SET @@` / `SET SESSION` if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-core/src/**` (session overlay map / reset)
- `crates/rusql-i18n/**` (errors only, e.g. SET GLOBAL)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- Enumerating every MySQL 8.0 sysvar
- `SET NAMES` / user-defined `@foo`
- `SET TRANSACTION ISOLATION LEVEL` (unless already a one-line parse wire-up to `transaction_isolation`)
- Persisting variables across server restart
- New crates

## Negative Constraints

- Do not implement MySQL read-only vs dynamic metadata for the full catalog; only the documented stub names are assignable overlays (read-only names such as `version` / `license` / `system_time_zone` may reject SET — document which)
- Do not change autocommit engine behavior (still autocommit-on for DML unless already session-aware); this slice is probe/persistence of the stub value
- Do not implement `SET GLOBAL` persistence

## Test plan

```bash
cargo test -p rusql-executor set_session_var
cargo test -p rusql-server set_session_var
```
