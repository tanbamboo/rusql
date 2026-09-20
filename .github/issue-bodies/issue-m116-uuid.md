## Goal

Evaluate `UUID()` so clients that generate identifiers no longer get `unsupported function`.

## Background

Phase R. Probe: `fn_uuid`. MySQL `UUID()` returns a string like `aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee` (time-based v1). rusql may implement RFC 4122 v4 if documented; `mysql-diff` for UUID must use `compare_output: false` because values differ.

## Acceptance Criteria

- [ ] `SELECT UUID()` returns one string cell matching MySQL 8-4-4-4-12 hex form with hyphens
- [ ] Two calls on the same connection are not equal
- [ ] `UUID()` takes no arguments; extra args error
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` plus shape/format asserts in unit tests; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF, rusql-vs-mysql

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- `UUID_TO_BIN` / `BIN_TO_UUID` / `UUID_SHORT`
- New crates (use std or existing deps)
- WAL changes

## Negative Constraints

- Do not require `mysql-diff` cell equality vs Docker MySQL
- Do not implement `SYS_GUID`

## Test plan

```bash
cargo test -p rusql-executor uuid
cargo test -p rusql-server uuid
```
