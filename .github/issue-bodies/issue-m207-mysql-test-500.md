## Goal

Grow the portable `mysql-test` subset toward 500 cases (from 100+).

## Category

Phase Z — Remaining MySQL 8.0 surface (M207). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M60 100+ portable cases. M30 subset runner exists. Add cases that already pass or will pass after R–Y; SKIPS for the rest with reason.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] CI mysql-test subset lists ≥ 500 included tests OR documents the count increase vs current with a floor (e.g. +400) if 500 is not yet portable — prefer real 500
- [ ] SKIPS.md updated; no silent omit
- [ ] `node scripts/mysql-test-subset.mjs` green

## File Boundaries

Allowed:
- `crates/rusql-server/src/mysql_test_subset.rs`
- `tests/mysql-test/**`
- `scripts/mysql-test-subset.mjs`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Lowering sensors to pass
- Oracle MTR as a required gate without portable filter

## Negative Constraints

- Do not add tests that require DELIMITER until M172
- Do not duplicate mysql-diff JSON as MTR

## Test plan

```bash
node scripts/mysql-test-subset.mjs
```
