## Goal

Implement `SELECT DISTINCT` so duplicate result rows are removed (MySQL semantics), including with `ORDER BY` / `LIMIT`.

## Background

Phase Q after M66. DISTINCT is a common ORM/reporting gap. UNION already dedupes via `dedupe_rows`; SELECT DISTINCT should reuse that path.

## Acceptance Criteria

- [ ] `SELECT DISTINCT col FROM t` returns unique values
- [ ] Multi-column DISTINCT compares full projected rows
- [ ] Works with `WHERE`, `ORDER BY`, `LIMIT`
- [ ] Works with JOINs and expression projections (e.g. `SELECT DISTINCT IF(...)`)
- [ ] `DISTINCT ON (...)` returns a clear unsupported error (PostgreSQL syntax)
- [ ] Unit/wire tests + `mysql-diff` suite
- [ ] Docs: CHANGELOG, release-notes, user-guide (en+zh-CN), roadmap Phase Q

## File Boundaries

Allowed:
- `crates/rusql-executor/src/lib.rs`
- `crates/rusql-server/src/**` (tests)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Storage redesign
- DISTINCT ON implementation
- New crates

## Negative Constraints

- Do not change UNION dedupe semantics
- Do not implement `SELECT ALL` beyond current default

## Test plan

```bash
cargo test -p rusql-executor distinct
cargo test -p rusql-server select_distinct
node scripts/mysql-diff.mjs
```
