## Goal

Accept `SHOW ENGINE INNODB STATUS` as a documented stub result so DBA/GUI probes are not unsupported.

## Background

M88 `SHOW ENGINES` lists InnoDB as DEFAULT. Probe: `show_engine_innodb_status`. MySQL returns one row with a large `Status` text. rusql: documented stub text (not live mutex/lock stats — M193).

## Acceptance Criteria

- [ ] `SHOW ENGINE INNODB STATUS` returns MySQL-shaped columns (`Type`, `Name`, `Status`) with a non-empty stub `Status` cell
- [ ] Unknown engine name is an error (document errno)
- [ ] `SHOW ENGINES` unchanged
- [ ] Unit/wire tests; `mysql-diff` `compare_output: false`; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF, rusql-vs-mysql

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**` (rewrite)
- `crates/rusql-executor/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Live InnoDB metrics (M193)
- WAL format
- Claiming crash-recovery compatibility

## Negative Constraints

- Do not implement `SHOW ENGINE PERFORMANCE_SCHEMA STATUS` unless one-line stub

## Test plan

```bash
cargo test -p rusql-sql engine_innodb
cargo test -p rusql-executor engine_innodb
cargo test -p rusql-server engine_innodb
```
