## Goal

Evaluate `TRIM`/`LTRIM`/`RTRIM`, `REPLACE`, and `SUBSTRING_INDEX` so common string SQL is not unsupported.

## Category

Phase S — JSON, set SQL, and remaining query forms (M141). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M113 SUBSTRING exists. Apps use TRIM/REPLACE and SUBSTRING_INDEX for CSV-ish fields. MySQL TRIM supports BOTH/LEADING/TRAILING [remstr] FROM str.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `TRIM('  a  ')` → `a`; `TRIM(LEADING 'x' FROM 'xxa')` → `a`
- [ ] `REPLACE('abc', 'b', 'x')` → `axc`. Empty search string: pin MySQL behavior
- [ ] `SUBSTRING_INDEX('a.b.c', '.', 2)` → `a.b`; negative count from the right
- [ ] SUBSTRING/ROUND/DATE_ADD unchanged
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
- `REGEXP` / `RLIKE`
- Collation-aware REPLACE beyond existing utf8mb4 compare
- WAL

## Negative Constraints

- Do not implement `INSERT(str,pos,len,new)` function (name clash with DML)
- Do not add `MID` unless alias of SUBSTRING

## Test plan

```bash
cargo test -p rusql-executor trim
cargo test -p rusql-executor replace_fn
cargo test -p rusql-executor substring_index
```
