## Goal

Optimize primary-key `UPDATE` path to match MySQL 8.0 throughput on durable commits.

## Category

Performance — storage / DML hot path.

## Depends on

- PERF-B1 persistent-connection harness
- M31 durable WAL, M32 MVCC (done)

## Acceptance Criteria

- [ ] On PERF-B1 harness: `update_pk` QPS ≥ **90%** of MySQL 8.0 (500 iterations, same row)
- [ ] p95 latency ≤ 1.25× MySQL
- [ ] Correctness: MVCC readers see consistent snapshots during update load test
- [ ] Profile notes (brief) in PR: where time was spent before/after

## File Boundaries

- `crates/rusql-storage/**`, `crates/rusql-executor/**`, `crates/rusql-server/**`

## Negative Constraints

- Do not disable durable COMMIT by default to win benchmark
- Optional `--wal-sync=none` bench flag OK if documented separately (see PERF-B5)

## Baseline (2026-08-11 CLI bench)

| Engine | QPS | Avg ms |
|--------|-----|--------|
| rusql | 34.76 | 28.8 |
| MySQL | 55.77 | 17.9 |
| Ratio | **0.62×** | |

Target: **≥0.90×** on PERF-B1 harness.
