## Goal

Measure rusql vs MySQL under concurrent clients (1, 4, 8, 16 threads).

## Category

Performance — concurrency characterization.

## Depends on

- PERF-B1 persistent-connection harness

## Acceptance Criteria

- [ ] Harness accepts `--threads N` and `--duration SEC`
- [ ] Reports aggregate and per-thread QPS for read-heavy and write-heavy mixes
- [ ] Baseline document section: rusql/MySQL ratio at each concurrency level
- [ ] Identifies lock/contention bottleneck if rusql scales sub-linearly

## File Boundaries

- `scripts/**`, `docs/en/reports/**`

## Negative Constraints

- No cluster/multi-instance test in v1
- Write mix limited to supported SQL (single-row INSERT/UPDATE)

## Expected outcome

Establish whether MVCC + single-writer storage limits speedup vs MySQL InnoDB.
