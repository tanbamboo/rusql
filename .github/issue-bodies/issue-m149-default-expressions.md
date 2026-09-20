## Goal

Evaluate `DEFAULT (expr)` (and existing literal defaults) when INSERT omits the column.

## Category

Phase T — Schema completeness (M149). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Literal DEFAULT exists. MySQL 8.0.13+ `DEFAULT (expr)` e.g. `DEFAULT (UUID())` or `DEFAULT (a+1)`. This slice: deterministic expr using existing builtins; UUID() only if M116 is on main, else skip UUID defaults in tests.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `n INT DEFAULT (1+2)` then INSERT omit-n stores `3`
- [ ] `DEFAULT CURRENT_TIMESTAMP` on DATETIME/TIMESTAMP if not already working — pin tests
- [ ] Invalid DEFAULT expr at CREATE time errors. Additive catalog
- [ ] Unit/wire tests; `mysql-diff`; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-core/src/**`
- `crates/rusql-storage/src/**` (additive)
- `crates/rusql-executor/src/**` (insert)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Generated columns (M146) except DEFAULT vs AS distinction
- Volatile default side effects beyond UUID/NOW already implemented
- Breaking WAL

## Negative Constraints

- Do not evaluate DEFAULT on UPDATE unless `DEFAULT` keyword is in the SET list
- Do not add DEFAULT on PRIMARY KEY separately from AUTO_INCREMENT

## Test plan

```bash
cargo test -p rusql-executor default_expr
cargo test -p rusql-sql default_expr
```
