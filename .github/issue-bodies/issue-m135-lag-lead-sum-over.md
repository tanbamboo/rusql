## Goal

Evaluate `LAG`/`LEAD` and `SUM() OVER (PARTITION BY … ORDER BY …)` without frames unless M129 already landed.

## Category

Phase S — JSON, set SQL, and remaining query forms (M135). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M70 ranking windows exist (`ROW_NUMBER`/`RANK`/`DENSE_RANK`). Analytics SQL needs offset windows and running SUM. Default offset is 1; default for NULL is SQL NULL.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `LAG(col, 1)` / `LEAD(col)` with `OVER (ORDER BY id)` match MySQL on a 3-row fixture (first LAG NULL, last LEAD NULL)
- [ ] `SUM(n) OVER (PARTITION BY g ORDER BY id)` is a running sum per partition (no `ROWS` clause required)
- [ ] If M129 is not on `main`, `ROWS BETWEEN` remains unsupported (do not sneak frames in)
- [ ] Existing ranking functions unchanged
- [ ] Unit/wire tests; `mysql-diff`; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-executor/src/window.rs`
- `crates/rusql-executor/src/expr.rs`
- `crates/rusql-sql/src/**` if parse rewrite needed
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Window `RANGE BETWEEN` / named frames (M203)
- Changing M70 ranking semantics
- New crates

## Negative Constraints

- Do not implement `NTH_VALUE` / `CUME_DIST` in this slice
- Do not require frames for SUM OVER

## Test plan

```bash
cargo test -p rusql-executor lag_lead
cargo test -p rusql-executor sum_over
```
