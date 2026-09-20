## Goal

Accept `CREATE OR REPLACE VIEW` so view deploy scripts are not `OR REPLACE VIEW is not supported`.

## Background

M33 `CREATE VIEW` exists; executor currently errors on `or_replace`. Probe: `create_or_replace_view`.

## Acceptance Criteria

- [ ] `CREATE OR REPLACE VIEW gap_vw AS SELECT id FROM gap_v` creates the view if missing
- [ ] Repeating the statement replaces the stored SELECT; `SHOW CREATE VIEW` / querying the view uses the new SQL
- [ ] Replacing a **base table** of the same name is an error (MySQL: cannot replace a table with a view — pin errno)
- [ ] Plain `CREATE VIEW` without OR REPLACE still errors on duplicate (existing M33)
- [ ] Unit/wire tests; `mysql-diff`; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF, rusql-vs-mysql

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-core/src/**` (view catalog only if needed)
- `crates/rusql-storage/src/**` only if view persistence already uses storage
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Materialized views
- ALGORITHM / DEFINER / SQL SECURITY
- WAL format changes unless views already WAL-logged (additive only)

## Negative Constraints

- Do not implement `ALTER VIEW` unless it is the same replace path with tests

## Test plan

```bash
cargo test -p rusql-executor or_replace_view
cargo test -p rusql-server or_replace_view
```
