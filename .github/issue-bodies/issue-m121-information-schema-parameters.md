## Goal

Expose `information_schema.PARAMETERS` so stored-program metadata queries are not errno 1146.

## Background

Phase R. Probe: `information_schema_parameters`. Procedures/functions currently have empty parameter lists in SHOW CREATE (M95/M96). This slice: virtual table that is empty until M132 adds `IN` params, **or** lists rows once params exist. Must not 1146.

## Acceptance Criteria

- [ ] `SELECT SPECIFIC_NAME FROM information_schema.PARAMETERS LIMIT 1` is a valid query (0 rows OK if no params catalogued)
- [ ] Not errno 1146. Documented columns include `SPECIFIC_NAME`, `PARAMETER_MODE`, `PARAMETER_NAME`, `DATA_TYPE` (subset OK)
- [ ] After M132, `IN` params appear here (this issue may land first with empty catalog)
- [ ] Unit/wire tests; `mysql-diff` `compare_output: false` if column extras differ; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF, rusql-vs-mysql

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**` (`info_schema.rs`)
- `crates/rusql-core/src/programs.rs` (read-only param metadata if already stored)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Implementing `IN`/`OUT` parsing (M132) unless already trivial
- WAL format
- Inventing fake parameters

## Negative Constraints

- Do not change SHOW CREATE PROCEDURE empty param list until M132

## Test plan

```bash
cargo test -p rusql-executor parameters
cargo test -p rusql-server parameters
```
