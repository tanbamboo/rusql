## Goal

Accept `EXCEPT` and `EXCEPT ALL` with the same column-count/type rules as M44 `UNION`.

## Category

Phase S — JSON, set SQL, and remaining query forms (M136). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M44 UNION / UNION ALL and M128 INTERSECT. Set-difference is the remaining set op for MySQL 8.0.1+ (`EXCEPT` = `MINUS` synonym optional).


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `SELECT 1 EXCEPT SELECT 1` returns zero rows; `SELECT 1 EXCEPT SELECT 2` returns `1`
- [ ] `EXCEPT ALL` preserves duplicates per MySQL (pin a 2-row left vs 1-row right fixture)
- [ ] Column count mismatch errors (same class as UNION). INTERSECT/UNION unchanged
- [ ] Unit/wire tests; `mysql-diff`; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-executor/src/**` (set ops / UNION path)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- `MINUS` as a second parser unless it aliases EXCEPT
- Changing UNION NULL-padding rules except to match EXCEPT
- WAL / charset

## Negative Constraints

- Do not implement `EXCEPT DISTINCT` as a separate keyword unless sqlparser requires it (EXCEPT is already distinct)
- Do not add INTERSECT ALL unless already in M128

## Test plan

```bash
cargo test -p rusql-executor except
cargo test -p rusql-server except
```
