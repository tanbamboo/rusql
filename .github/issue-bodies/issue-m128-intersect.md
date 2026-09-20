## Goal

Evaluate `INTERSECT` (distinct) so set-SQL matching the probe is not unsupported.

## Background

M44 `UNION` / `UNION ALL` exist. Probe: `intersect`. `EXCEPT` is Phase S (M136).

## Acceptance Criteria

- [ ] `SELECT 1 AS n INTERSECT SELECT 1 AS n` returns one row `1`
- [ ] `SELECT 1 AS n INTERSECT SELECT 2 AS n` returns zero rows
- [ ] Column count mismatch is an error. `UNION` unchanged
- [ ] Unit/wire tests; `mysql-diff`; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF, rusql-vs-mysql

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**` (set ops)
- `crates/rusql-sql/src/**` if parse needs help
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- `EXCEPT` (M136)
- WAL format
- `INTERSECT ALL` unless tests pin it as a one-liner

## Negative Constraints

- Do not change UNION ALL duplicate semantics

## Test plan

```bash
cargo test -p rusql-executor intersect
cargo test -p rusql-server intersect
```
