## Goal

Accept `CREATE TABLE … LIKE` (copy schema) and `CREATE TABLE … AS SELECT` (schema+rows).

## Category

Phase T — Schema completeness (M154). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Common ETL/dump patterns. LIKE copies columns/indexes/defaults of a base table (not data). CTAS creates columns from SELECT and inserts rows. Skip generated STORED/partition copies if those issues are not on main.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `CREATE TABLE t2 LIKE t1` copies columns and PK; t2 starts empty
- [ ] `CREATE TABLE t3 AS SELECT id, n FROM t1` has those columns and the selected rows
- [ ] Unknown source table errno 1146. CREATE TABLE IF NOT EXISTS interaction documented
- [ ] Unit/wire tests; `mysql-diff`; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-executor/src/**` (DDL)
- `crates/rusql-storage/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- CREATE TABLE … LIKE views (error or document)
- Tablespace clauses
- Breaking WAL

## Negative Constraints

- Do not copy FOREIGN KEY unless cheap reuse of M39
- Do not copy partitioning (M152)

## Test plan

```bash
cargo test -p rusql-executor create_table_like
cargo test -p rusql-executor create_table_as_select
```
