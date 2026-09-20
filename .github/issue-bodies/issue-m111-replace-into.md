## Goal

Accept `REPLACE INTO` as delete-then-insert on primary-key conflict so dump/ORM upserts that use REPLACE succeed.

## Background

Phase Q DML. `INSERT … ON DUPLICATE KEY UPDATE` (M68) already upserts on PRIMARY KEY. Probe (`scripts/mysql-gap-probe.mjs`, 2026-09-20): `replace_into` → 1105 `REPLACE INTO is not supported`. MySQL REPLACE deletes the old row (fires DELETE+INSERT triggers) then inserts. This slice: on PK conflict, replace the row; affected_rows may be 1 (insert) or 2 (delete+insert) to match MySQL’s common client-visible count. Secondary UNIQUE is later if not already in M68.

## Acceptance Criteria

- [ ] `REPLACE INTO t VALUES (…)` inserts when the PK is new
- [ ] `REPLACE INTO t VALUES (…)` with an existing PK replaces that row (old row gone, new values stored)
- [ ] `INSERT … ON DUPLICATE KEY UPDATE` from M68 is unchanged
- [ ] Unit/wire tests; `mysql-diff` can compare selected rows after REPLACE; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (only if a one-line rewrite is required)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- `INSERT IGNORE` (M112)
- Multi-table REPLACE
- New crates
- Inventing UNIQUE-not-PK conflict handling beyond what M68 already does

## Negative Constraints

- Do not change ODKU semantics
- Do not require DELETE/INSERT trigger firing unless it is a one-line reuse of existing trigger hooks
- Do not implement GTID event type 33 / heartbeat

## Test plan

```bash
cargo test -p rusql-executor replace
cargo test -p rusql-server replace
```
