## Goal

Accept `INSERT IGNORE` so duplicate-key inserts skip the conflicting row instead of errno 1062 / `INSERT IGNORE is not supported`.

## Background

Phase Q DML after M111. Probe (`scripts/mysql-gap-probe.mjs`, 2026-09-20): `insert_ignore` → 1105 `INSERT IGNORE is not supported`. Existing unit test `insert_ignore_is_unsupported` must be replaced. MySQL `INSERT IGNORE` skips rows that would violate PK/UNIQUE and still inserts the rest of a multi-row insert. This slice: PRIMARY KEY conflict only (same unique surface as M68). `affected_rows` is the number of rows actually inserted.

## Acceptance Criteria

- [ ] `INSERT IGNORE INTO t VALUES (existing_pk, …)` returns OK and does not change the existing row
- [ ] `INSERT IGNORE` of a new PK inserts as usual
- [ ] Multi-row `INSERT IGNORE` inserts non-conflicting rows and skips PK conflicts
- [ ] Plain `INSERT` of a duplicate PK remains errno 1062. M68 ODKU and M111 REPLACE are unchanged
- [ ] Unit/wire tests; `mysql-diff`; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (only if needed to preserve IGNORE flag)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- `REPLACE INTO` (M111)
- Warning generation (`SHOW WARNINGS` live 1062 notes)
- New crates

## Negative Constraints

- Do not silently ignore non-PK errors (unknown column, type errors still fail)
- Do not implement sql_mode-dependent IGNORE for truncation
- Do not implement GTID event type 33 / heartbeat

## Test plan

```bash
cargo test -p rusql-executor insert_ignore
cargo test -p rusql-server insert_ignore
```
