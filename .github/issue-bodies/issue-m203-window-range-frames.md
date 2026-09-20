## Goal

Named windows and `RANGE BETWEEN` frames after M129 `ROWS BETWEEN`.

## Category

Phase Z — Remaining MySQL 8.0 surface (M203). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M70 ranking; M129 ROWS; M135 LAG/SUM. RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW on numeric/date ORDER BY.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `WINDOW w AS (PARTITION BY g ORDER BY id ROWS BETWEEN …)` reusable in SELECT list
- [ ] `RANGE BETWEEN INTERVAL 1 DAY PRECEDING AND CURRENT ROW` or numeric RANGE — pin one portable fixture vs MySQL
- [ ] M129 ROWS unchanged. Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-executor/src/window.rs`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- GROUPS frames
- EXCLUDE TIES

## Negative Constraints

- Do not implement RANGE on strings
- Do not change default frame for RANK

## Test plan

```bash
cargo test -p rusql-executor window_range
cargo test -p rusql-executor named_window
```
