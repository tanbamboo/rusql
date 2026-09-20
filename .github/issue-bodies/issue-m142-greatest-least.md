## Goal

Evaluate `GREATEST`/`LEAST` with two or more arguments (IF/IFNULL already exist).

## Category

Phase S — JSON, set SQL, and remaining query forms (M142). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M46/M66 cover IF/IFNULL/COALESCE. GREATEST/LEAST are the remaining comparison builtins ORMs emit. NULL handling: MySQL returns NULL if any arg is NULL (default sql_mode).


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `GREATEST(1, 3, 2)` → `3`; `LEAST(1, 3, 2)` → `1`
- [ ] `GREATEST(1, NULL)` is SQL NULL (document if sql_mode would differ)
- [ ] Fewer than two arguments errors. String vs numeric: pin MySQL coercion in tests or reject mixed types with a documented errno
- [ ] Unit/wire tests; `mysql-diff`; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-executor/src/expr.rs`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- `BETWEEN SYMMETRIC`
- Changing COALESCE/IF
- WAL

## Negative Constraints

- Do not implement `GREATEST` as a window function
- Do not add `MIN`/`MAX` scalar over varargs beyond GREATEST/LEAST

## Test plan

```bash
cargo test -p rusql-executor greatest_least
cargo test -p rusql-server greatest_least
```
