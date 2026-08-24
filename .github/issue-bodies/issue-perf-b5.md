## Goal

Expose configurable WAL durability (`fsync` policy) and document throughput/latency trade-offs vs MySQL `innodb_flush_log_at_trx_commit`.

## Category

Performance — storage durability tuning.

## Depends on

- M31 durable WAL, PERF-B1 harness

## Acceptance Criteria

- [ ] Server flag e.g. `--wal-sync=always|batch|none` (names documented)
- [ ] PERF-B1 shows QPS/latency matrix for each mode on `begin_commit` workload
- [ ] Default remains safe (MySQL `=1` equivalent)
- [ ] user-guide explains data-loss risk for non-default modes

## File Boundaries

- `crates/rusql-storage/**`, `crates/rusql-server/**`, `docs/en/user-guide.md`, `crates/rusql-i18n/**`

## Negative Constraints

- Do not change default durability to improve benchmarks silently
- No group commit required in v1 but not blocked if simple

## Baseline

`begin_commit` rusql 0.92× MySQL on CLI bench — revisit after sync policy options.
