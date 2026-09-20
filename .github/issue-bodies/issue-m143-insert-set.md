## Goal

Accept `INSERT INTO t SET c=expr, …` as equivalent to column-list INSERT.

## Category

Phase S — JSON, set SQL, and remaining query forms (M143). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Column-list and VALUES INSERT exist. MySQL `INSERT … SET` is common in dumps and some ORMs. Same conflict handling as plain INSERT (ODKU/IGNORE stay their own issues).


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `INSERT INTO t SET id=1, n=2` inserts one row equal to `INSERT INTO t (id, n) VALUES (1, 2)`
- [ ] Unknown column errno 1054. Duplicate PK errno 1062 (same as INSERT)
- [ ] INSERT IGNORE / ODKU / REPLACE unchanged
- [ ] Unit/wire tests; `mysql-diff`; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-executor/src/**` (insert)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Multi-table INSERT
- INSERT SELECT changes (M68)
- WAL format

## Negative Constraints

- Do not implement `INSERT SET` + `ON DUPLICATE` unless it falls out of existing ODKU for free — still test it or document skip
- Do not change DEFAULT expr (M149)

## Test plan

```bash
cargo test -p rusql-sql insert_set
cargo test -p rusql-executor insert_set
```
