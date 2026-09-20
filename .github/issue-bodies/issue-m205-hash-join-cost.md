## Goal

Planner may choose hash join or BNL with documented cost vs nested loop.

## Category

Phase Z — Remaining MySQL 8.0 surface (M205). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M49 nested-loop cost. Large joins need hash join. EXPLAIN shows join type. Correctness first; cost second.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] A two-table join fixture uses hash join when nested loop would be worse (EXPLAIN or trace)
- [ ] Result set matches nested-loop join (mysql-diff)
- [ ] INNER JOIN correctness unchanged. Docs as usual

## File Boundaries

Allowed:
- `crates/rusql-planner/src/**`
- `crates/rusql-executor/src/**` (join)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- FULL OUTER hash join unless M137 exists and tests pin it
- Changing isolation

## Negative Constraints

- Do not implement GRACE hash spill unless tests require
- Do not add optimizer hints except optional comment ignore

## Test plan

```bash
cargo test -p rusql-planner hash_join
cargo test -p rusql-executor hash_join
```
