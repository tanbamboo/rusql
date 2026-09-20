## Goal

Store `ENUM`/`SET` as strings with documented invalid-value behavior under current sql_mode.

## Category

Phase T — Schema completeness (M148). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M40 extended types. ENUM/SET appear in dumps. Store the member string; reject or coerce empty/invalid per documented sql_mode (prefer reject errno 1265/1366 for strict).


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `CREATE TABLE t (id INT PRIMARY KEY, e ENUM('a','b'))` then `INSERT … 'a'` succeeds; `'c'` errors in strict mode (document errno)
- [ ] `SET('a','b')` stores canonical MySQL comma lists for `'a,b'` (order = declaration order)
- [ ] SHOW CREATE TABLE reconstructs ENUM/SET member lists
- [ ] Unit/wire tests; `mysql-diff`; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-core/src/types.rs`
- `crates/rusql-storage/src/**` (additive type meta)
- `crates/rusql-executor/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Integer ENUM index storage as a second on-disk format unless tests require it — string is enough
- New collations
- Breaking WAL

## Negative Constraints

- Do not implement ENUM in CAST beyond storing/validating members
- Do not add SET functions (`FIND_IN_SET`) unless one test needs it — prefer a follow-up

## Test plan

```bash
cargo test -p rusql-executor enum_type
cargo test -p rusql-executor set_type
```
