## Goal

Add optional CI/local gate running Sysbench `oltp_point_select` against rusql and MySQL reference.

## Category

Performance — industry-standard OLTP read benchmark.

## Depends on

- M61 Sysbench-compatible schema, PERF-B1 harness patterns

## Acceptance Criteria

- [ ] CI workflow (optional/manual `workflow_dispatch`) runs Sysbench point-select
- [ ] Fails if rusql QPS < **70%** of MySQL on same host (configurable threshold)
- [ ] Results appended to benchmark report template
- [ ] Document install: Sysbench + Docker MySQL in dev guide

## File Boundaries

- `.github/workflows/**`, `scripts/**`, `docs/en/reports/**`

## Negative Constraints

- Do not block PR CI on Sysbench if tools missing — soft gate or nightly only
- Full `oltp_read_write` out of scope until M37/M45+ merged

## Reference

Oracle Sysbench docs: https://dev.mysql.com/doc/refman/8.0/en/sysbench.html
