## Goal

Provide schema and seed scripts compatible with Sysbench `oltp_*` workloads (minimum: point select).

## Category

Phase P — Compat harness expansion.

## Depends on

- M40 extended types, M37 AUTO_INCREMENT (Sysbench tables use AUTO_INCREMENT)

## Acceptance Criteria

- [ ] Documented DDL matching Sysbench `sbtest` table shape (adapted to rusql subset)
- [ ] `scripts/sysbench-rusql.mjs` or Makefile target runs `oltp_point_select` against rusql + MySQL
- [ ] README section: how to run Sysbench comparison locally
- [ ] Blocked workloads listed with issue links (writes, complex OLTP)

## File Boundaries

- `scripts/**`, `docs/en/user-guide.md`, `docs/zh-CN/user-guide.md`

## Negative Constraints

- Do not require Sysbench in CI if Docker unavailable — document optional job
- Full `oltp_read_write` deferred until SQL surface ready
