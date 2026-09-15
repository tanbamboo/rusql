## Goal

Implement a documented set of MySQL `@@` session/system variables so clients, JDBC/ORMs, and the official CLI can run `SELECT @@version`, `@@autocommit`, charset, and related probes without `unsupported expression` errors.

## Background

Phase Q after M76. `VERSION()`, `DATABASE()`, and a one-off identifier `@@version_comment` already work. Connectors still send `SELECT @@session.auto_increment_increment`, `@@character_set_client`, `@@sql_mode`, `@@autocommit`, etc. on connect. Full `SHOW VARIABLES` remains later.

## Acceptance Criteria

- [ ] `SELECT @@version` returns the same MySQL 8.0-compatible string as `VERSION()` (contains `8.0`)
- [ ] `SELECT @@version_comment` works as a `@@` variable (not only the existing identifier special-case) and is stable
- [ ] `SELECT @@autocommit` returns `1` (rusql default; matches current autocommit-on behavior)
- [ ] `SELECT @@sql_mode` returns a non-empty MySQL-like mode string (document the stub)
- [ ] `SELECT @@character_set_client`, `@@character_set_connection`, `@@character_set_results`, `@@character_set_server` return `utf8mb4`
- [ ] `SELECT @@collation_connection` returns a supported collation (`utf8mb4_unicode_ci` or `utf8mb4_0900_ai_ci` — document which)
- [ ] `@@session.var` and `@@var` are equivalent for this stub set
- [ ] Unknown `@@` names return a MySQL-like error (errno 1193 `Unknown system variable`) via i18n
- [ ] Unit/wire tests; `mysql-diff` where portable (`compare_output: false` for values that differ from Docker MySQL); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (parse `@@` / `@@session.` if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (unknown-variable error)
- `crates/rusql-core/src/**` (session fields if needed)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- Full `SHOW VARIABLES` catalog
- `SET @@` / user-defined variables `@foo`
- `FOUND_ROWS()` / `SQL_CALC_FOUND_ROWS`
- New crates

## Negative Constraints

- Do not implement `SET NAMES` / `SET @@autocommit=0` persistence in this issue unless already present and a one-line wire-up
- Do not enumerate every MySQL 8.0 sysvar; only the listed stub set plus a documented unknown-variable error
- Do not implement `FOUND_ROWS()` in this issue

## Test plan

```bash
cargo test -p rusql-executor session_var
cargo test -p rusql-server session_var
```
