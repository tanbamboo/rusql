## Goal

Add a reproducible benchmark harness using **persistent connections** (not per-query CLI spawn) to measure true server throughput.

## Category

Performance — baseline infrastructure. See [performance-benchmark-2026-08-11.md](../../docs/en/reports/performance-benchmark-2026-08-11.md).

## Depends on

- Supported SQL subset (current main)

## Acceptance Criteria

- [ ] `scripts/bench-rusql-vs-mysql.mjs` (or Rust binary) runs read/write workloads via persistent mysql2/Rust client
- [ ] Outputs JSON with QPS, p50/p95 latency per workload
- [ ] Documents hardware/env fields; writes to `docs/en/reports/` on CI/manual run
- [ ] Same 7 workloads as 2026-08-11 baseline for continuity
- [ ] README quick-start for benchmark

## File Boundaries

- `scripts/**`, `docs/en/reports/**`, `docs/en/user-guide.md`

## Negative Constraints

- Do not commit local `.bench-*.json` artifacts — gitignore or use `target/`
- Do not require MySQL Docker in default CI (optional workflow OK)

## Baseline reference

2026-08-11 CLI benchmark overstated relative gap due to process spawn; this harness is prerequisite for PERF-B2/B3 validation.
