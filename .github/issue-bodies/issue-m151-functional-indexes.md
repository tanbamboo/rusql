## Goal

Accept `INDEX ((expr))` and use it for a documented equality point lookup.

## Category

Phase T — Schema completeness (M151). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M50 composite indexes. Functional indexes are MySQL 8.0.13+. MVP: one expression, equality lookup, no prefix. Planner chooses it when WHERE expr = const.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `CREATE TABLE t (id INT PRIMARY KEY, n INT, INDEX ((n+1)))` then `SELECT * FROM t WHERE n+1 = 2` uses the functional index (EXPLAIN or equivalent test)
- [ ] INSERT maintains the index. Unknown functions in expr error at CREATE
- [ ] Unit/wire tests; docs as usual; mysql-diff if EXPLAIN shape is stable

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-core/src/**`
- `crates/rusql-storage/src/**`
- `crates/rusql-planner/src/**`
- `crates/rusql-executor/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- STORED generated-column indexes as a substitute without documenting
- Hash indexes
- Breaking WAL keys

## Negative Constraints

- Do not implement descending functional indexes
- Do not claim covering-index for expressions

## Test plan

```bash
cargo test -p rusql-storage functional_index
cargo test -p rusql-planner functional_index
```
