## Goal

Implement `SET NAMES` charset overlays and user-defined session variables `@foo` so connectors that send `SET NAMES utf8mb4` or `SET @x = …` after connect see the assignment on that same connection.

## Background

Phase Q after M81. `SET @@` overlays work for the documented stub catalog, but `SET NAMES` and `@foo` still error. Official CLI, JDBC, and several ORMs send `SET NAMES utf8mb4` (and sometimes `SET @foo = 0`) during handshake. Full charset conversion on the wire and `SELECT @foo := expr` assignment expressions remain later.

## Acceptance Criteria

- [ ] `SET NAMES utf8mb4` then `SELECT @@character_set_client`, `@@character_set_connection`, and `@@character_set_results` return `utf8mb4` on that connection (overlay; do not change packet encoding)
- [ ] `SET NAMES utf8mb4 COLLATE utf8mb4_unicode_ci` updates `@@collation_connection` to `utf8mb4_unicode_ci` on that connection
- [ ] `SET NAMES DEFAULT` restores documented charset/collation stubs
- [ ] `SET @foo = 1` then `SELECT @foo` returns `1` on that connection; another connection does not see `@foo`
- [ ] Unset `SELECT @bar` returns NULL (empty cell is acceptable if documented) rather than an unknown-column error
- [ ] Assignments are in-memory per session (not WAL). `COM_RESET_CONNECTION` / `COM_CHANGE_USER` clear user variables and restore `SET NAMES` defaults
- [ ] `SET @@` behavior from M81 is unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` where Docker MySQL charset/CLI encoding differs; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (parse `SET @foo` / `SELECT @foo` if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-core/src/**` (user-var map / reset)
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- Wire charset conversion / `latin1` vs `utf8mb4` packet recoding
- `SELECT @foo := expr` assignment expressions (unless already a one-line parse wire-up)
- `SET CHARACTER SET` as a separate dialect (unless a one-line alias of `SET NAMES`)
- Persisting variables across server restart
- New crates

## Negative Constraints

- Do not implement MySQL charset conversion of result bytes; this slice is probe/persistence of `SET NAMES` overlays plus `@foo` storage
- Do not enumerate every MySQL charset/collation; accept documented utf8mb4 collations already in rusql (`utf8mb4_0900_ai_ci`, `utf8mb4_unicode_ci`) and treat other names as opaque overlay strings
- Do not change autocommit engine behavior

## Test plan

```bash
cargo test -p rusql-executor set_names
cargo test -p rusql-executor user_var
cargo test -p rusql-server set_names
cargo test -p rusql-server user_var
```
