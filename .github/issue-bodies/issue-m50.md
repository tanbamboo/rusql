## Goal

Support multi-column and covering indexes for composite predicates.

## Category

Phase K — Query optimizer.

## Depends on

- M4 CREATE INDEX, M49 cost planner

## Acceptance Criteria

- [ ] `CREATE INDEX idx ON t (a, b)` persisted in catalog/WAL
- [ ] Queries filtering on `(a)` or `(a, b)` prefix use composite index
- [ ] `SHOW INDEX` reports multi-column `Seq_in_index`
- [ ] `information_schema.STATISTICS` matches MySQL column order
- [ ] Benchmark: composite lookup within 15% of MySQL on PERF-B1 harness

## File Boundaries

- `crates/rusql-storage/**`, `crates/rusql-core/**`, `crates/rusql-executor/**`

## Negative Constraints

- No R-tree/fulltext index types
