## Goal

Honor `INVISIBLE` columns (omitted from `SELECT *`) and optional invisible indexes as no-op planner flags if cheap.

## Category

Phase T — Schema completeness (M150). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

MySQL 8.0.23 invisible columns. Minimal: catalog flag + SELECT * skips them; explicit SELECT col still works. Invisible indexes: stored flag; planner may ignore (document).


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `CREATE TABLE t (id INT PRIMARY KEY, n INT INVISIBLE)` then `SELECT *` has one column `id`; `SELECT n` returns n
- [ ] INSERT must still supply n unless DEFAULT exists (MySQL requires a value for invisible without default — pin tests)
- [ ] SHOW CREATE TABLE includes `INVISIBLE`
- [ ] Unit/wire tests; `mysql-diff`; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-core/src/**`
- `crates/rusql-storage/src/**` (additive)
- `crates/rusql-executor/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Changing SELECT * for views beyond this flag
- Functional indexes (M151)
- Breaking WAL

## Negative Constraints

- Do not hide columns from information_schema.COLUMNS (MySQL still lists them)
- Do not implement `ALTER … SET INVISIBLE` unless reuse is obvious

## Test plan

```bash
cargo test -p rusql-executor invisible_column
cargo test -p rusql-server invisible_column
```
