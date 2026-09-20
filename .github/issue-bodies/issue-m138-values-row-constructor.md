## Goal

Accept MySQL `VALUES ROW(...)` (and/or table-value constructor) so row constructors are not parse errors.

## Category

Phase S — JSON, set SQL, and remaining query forms (M138). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

MySQL 8.0.19+ `VALUES ROW(1), ROW(2)` as a table. sqlparser may emit a different AST; rewrite to a UNION of one-row selects if needed. Pick the dialect Docker MySQL accepts.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `VALUES ROW(1), ROW(2)` as a top-level statement or in `FROM` returns two rows (column name pinned to MySQL, often `column_0`)
- [ ] Column count mismatch across ROW() errors. INSERT … VALUES (existing) unchanged
- [ ] Unit/wire tests; `mysql-diff`; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-executor/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Table-value constructor as CTE substitution for M69
- JSON constructors (M134)
- WAL

## Negative Constraints

- Do not implement `TABLE t` syntactic sugar
- Do not change INSERT row lists

## Test plan

```bash
cargo test -p rusql-sql values_row
cargo test -p rusql-executor values_row
```
