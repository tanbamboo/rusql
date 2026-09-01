## Goal

Grow portable mysql-test wire harness from 20 to 100+ cases with tracked pass rate.

## Category

Phase P — Compat harness expansion.

## Depends on

- M29 mysql-diff, M30 mysql-test subset

## Acceptance Criteria

- [ ] Extract ≥100 portable cases via `extract-mtr-sql.mjs` tagged by category
- [ ] CI job reports `passed/total` and fails on regression below floor (e.g. 95% of recorded)
- [ ] SKIPS.md updated with taxonomy for newly enabled categories
- [ ] Weekly compat % noted in release-notes template

## File Boundaries

- `tests/mysql-test/**`, `scripts/mysql-test-subset.mjs`, `scripts/extract-mtr-sql.mjs`, `docs/en/reports/**`

## Negative Constraints

- Do not vendor full Oracle mysql-test tree into repo
- Do not enable environment-specific cases without skip tags
