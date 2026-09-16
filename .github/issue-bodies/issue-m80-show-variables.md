## Goal

Implement `SHOW VARIABLES` / `SHOW SESSION VARIABLES` / `SHOW GLOBAL VARIABLES` so clients that introspect the documented M77+M79 stub catalog get `Variable_name`/`Value` rows instead of an unsupported-statement error.

## Background

Phase Q after M79. `SELECT @@var` covers the connector-probe stub set. The official CLI, JDBC, and several ORMs still send `SHOW VARIABLES` or `SHOW VARIABLES LIKE '…'` after connect. rusql must not enumerate the full MySQL 8.0 catalog (~500 names); return only the documented stub set. `SET @@` remains a later slice.

## Acceptance Criteria

- [ ] `SHOW VARIABLES` returns columns `Variable_name`, `Value` with one row per documented stub (M77 + M79 names, including `tx_isolation` for older connectors)
- [ ] `SHOW SESSION VARIABLES` is equivalent to `SHOW VARIABLES` for this set
- [ ] `SHOW GLOBAL VARIABLES` returns the same stub rows (document that session=global for this slice)
- [ ] `SHOW VARIABLES LIKE 'auto_increment%'` (MySQL `LIKE` pattern) filters that set; a non-matching pattern returns zero rows, not an error
- [ ] Stub values match `SELECT @@name` for the same names
- [ ] `SET @@` / user variables `@foo` remain unimplemented
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` for full `SHOW VARIABLES` (row counts differ from Docker MySQL); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (parse `SHOW [SESSION|GLOBAL] VARIABLES [LIKE …]` if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- Enumerating every MySQL 8.0 sysvar
- `SET @@` / `@foo` / `SET NAMES` persistence
- `SHOW STATUS` / `performance_schema`
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 `SHOW VARIABLES` parity; document the stub subset
- Do not implement `WHERE` filtering unless sqlparser already exposes it as a one-line wire-up (`LIKE` is required)
- Do not implement `SET SESSION` persistence in this issue

## Test plan

```bash
cargo test -p rusql-executor show_variables
cargo test -p rusql-server show_variables
```
