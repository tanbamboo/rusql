## Goal

Persist column histograms / table stats used by the planner (`ANALYZE TABLE`).

## Category

Phase Z — Remaining MySQL 8.0 surface (M204). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M49 cost planner. ANALYZE TABLE writes histogram; EXPLAIN may change. Persistence in catalog additive.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `ANALYZE TABLE t` succeeds and stores row-count/histogram subset
- [ ] Planner uses stats for at least one fixture (index vs scan documented)
- [ ] Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-planner/src/**`
- `crates/rusql-storage/src/**` (additive)
- `crates/rusql-executor/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- InnoDB persistent rec_per_key file format compatibility
- Hash join (M205) in the same PR unless already done

## Negative Constraints

- Do not sample 100% of huge tables in unit tests
- Do not claim MySQL histogram buckets identical

## Test plan

```bash
cargo test -p rusql-planner histogram
cargo test -p rusql-executor analyze_table
```
