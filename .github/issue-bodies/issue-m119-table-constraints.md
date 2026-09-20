## Goal

Expose `information_schema.TABLE_CONSTRAINTS` so ORM/migrator introspection is not errno 1146.

## Background

Phase R. Probe: `information_schema_table_constraints`. rusql already has PK / UNIQUE / FK metadata (`TableMeta`, `KEY_COLUMN_USAGE`). This slice is a virtual table listing constraints.

## Acceptance Criteria

- [ ] `SELECT CONSTRAINT_NAME FROM information_schema.TABLE_CONSTRAINTS WHERE TABLE_NAME = 'gap_tc'` returns a row for the PRIMARY KEY (MySQL name `PRIMARY`)
- [ ] UNIQUE and FOREIGN KEY constraints that already exist in the catalog appear with `CONSTRAINT_TYPE` `UNIQUE` / `FOREIGN KEY`
- [ ] Unknown `information_schema` tables still 1146. Existing `KEY_COLUMN_USAGE` unchanged
- [ ] Unit/wire tests; `mysql-diff` (constraint name can compare); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF, rusql-vs-mysql

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**` (`info_schema.rs` and dispatch)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Inventing CHECK constraints (M147)
- WAL format
- Full MySQL `TABLE_CONSTRAINTS` column set beyond a documented portable subset (must include `CONSTRAINT_NAME`, `TABLE_NAME`, `CONSTRAINT_TYPE`)

## Negative Constraints

- Do not add CHECK rows until M147
- Do not change `information_schema.EVENTS` (M109)

## Test plan

```bash
cargo test -p rusql-executor table_constraints
cargo test -p rusql-server table_constraints
```
