## Goal

Close throughput gap on `SELECT … ORDER BY k LIMIT n` vs MySQL 8.0.

## Category

Performance — query execution hot path.

## Depends on

- PERF-B1 persistent-connection harness
- M17 ORDER BY (done)

## Acceptance Criteria

- [ ] On PERF-B1 harness, 10k-row `bench_t`: QPS ≥ **90%** of MySQL 8.0 for `scan_order_limit` workload
- [ ] p95 latency ≤ 1.2× MySQL on same fixture
- [ ] No regression on point_select_pk / index_lookup (>95% of prior rusql QPS)
- [ ] Unit/integration test for sort+limit correctness preserved

## File Boundaries

- `crates/rusql-executor/**`, `crates/rusql-storage/**`, `crates/rusql-planner/**`

## Negative Constraints

- No change to SQL semantics for ORDER BY stability
- Avoid full in-memory sort when limit pushdown possible

## Baseline (2026-08-11 CLI bench)

| Engine | QPS | Avg ms |
|--------|-----|--------|
| rusql | 35.93 | 27.8 |
| MySQL | 48.43 | 20.6 |
| Ratio | **0.74×** | |

Target after optimization: **≥0.90×** on PERF-B1 harness.
