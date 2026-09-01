## Goal

Replace pass-through planner with index-aware cost-based plan selection.

## Category

Phase K — Query optimizer.

## Depends on

- M4 indexes, M50 composite indexes (can ship basic version before M50)

## Acceptance Criteria

- [ ] `EXPLAIN SELECT …` returns MySQL-shaped text/table (id, type, key, rows, Extra stub)
- [ ] Point lookup uses PK/unique index when available
- [ ] Range predicate `WHERE k BETWEEN …` uses index when selective
- [ ] Avoid full table scan when single-column index exists
- [ ] Regression test: plan picks index on 10k-row fixture

## File Boundaries

- `crates/rusql-planner/**`, `crates/rusql-executor/**`, `crates/rusql-server/**`

## Negative Constraints

- No join reordering beyond left-deep heuristic
- No histogram/statistics auto-analyze

## Performance link

Target improvement for scan+sort path (PERF-B2) after planner can push LIMIT.
