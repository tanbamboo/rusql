## Goal

Execute `WITH RECURSIVE` CTEs for the probe shape (counting UNION ALL) so hierarchical queries are not rejected.

## Background

M69 non-recursive `WITH` is inlined. Probe: `with_recursive`. Need a recursion cap (document, e.g. `cte_max_recursion_depth` stub 1000) to avoid infinite loops.

## Acceptance Criteria

- [ ] `WITH RECURSIVE cte AS (SELECT 1 AS n UNION ALL SELECT n + 1 FROM cte WHERE n < 3) SELECT n FROM cte` returns `1,2,3` (order documented)
- [ ] Recursion exceeding the cap is an error (i18n), not a hang
- [ ] Non-recursive `WITH` (M69) unchanged
- [ ] Unit/wire tests; `mysql-diff`; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF, rusql-vs-mysql

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-planner/src/**` if needed
- `crates/rusql-sql/src/**` only if parser flags RECURSIVE
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Cycle detection beyond the cap
- `SEARCH` / `CYCLE` clauses
- WAL format

## Negative Constraints

- Do not implement recursive DML
- Do not remove the existing RECURSIVE reject until this slice replaces it

## Test plan

```bash
cargo test -p rusql-executor recursive
cargo test -p rusql-server recursive
```
