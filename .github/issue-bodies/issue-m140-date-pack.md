## Goal

Evaluate a portable date pack: `DATE_SUB`, `DATEDIFF`, and `DATE_FORMAT` subset used by apps after M113 `DATE_ADD`.

## Category

Phase S — JSON, set SQL, and remaining query forms (M140). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M113 DATE_ADD with INTERVAL units. Remaining high-ROI: subtract, day difference, and a small format set (`%Y-%m-%d`, `%Y-%m-%d %H:%i:%s`). MONTH/YEAR keep M105 30/365-day approximation unless calendar add is already documented for DATE_ADD.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `DATE_SUB('2026-01-02', INTERVAL 1 DAY)` returns `2026-01-01` (same type rules as DATE_ADD)
- [ ] `DATEDIFF('2026-01-02', '2026-01-01')` returns `1` (date-only difference, MySQL sign)
- [ ] `DATE_FORMAT('2026-01-02', '%Y-%m-%d')` returns `2026-01-02`. Unsupported format specifiers: document or error consistently
- [ ] DATE_ADD unchanged. `STR_TO_DATE` not required
- [ ] Unit/wire tests; `mysql-diff`; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-executor/src/expr.rs`
- `crates/rusql-sql/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- `TIMESTAMPDIFF` / `EXTRACT` unless one-line wrappers of DATEDIFF with tests
- Calendar leap-month rewrite of M113
- WAL

## Negative Constraints

- Do not implement strftime-complete DATE_FORMAT
- Do not change event-scheduler interval math except by reuse

## Test plan

```bash
cargo test -p rusql-executor date_sub
cargo test -p rusql-executor datediff
cargo test -p rusql-executor date_format
```
