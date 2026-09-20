## Goal

Accept `EXPLAIN FORMAT=JSON` with a documented JSON subset so connectors that probe it do not error.

## Category

Phase S — JSON, set SQL, and remaining query forms (M145). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M49 EXPLAIN exists (text). Connectors send `FORMAT=JSON`. A stub object with query_block / table / access_type is enough if documented; live cost numbers are M49/M205.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `EXPLAIN FORMAT=JSON SELECT …` returns one JSON cell (not unsupported). Shape documented (query_block required)
- [ ] Plain `EXPLAIN` text output unchanged
- [ ] `EXPLAIN ANALYZE` may remain unsupported (error, not silent wrong plan)
- [ ] Unit/wire tests; docs as usual. mysql-diff optional if JSON key order differs — compare parsed keys

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-planner/src/**`
- `crates/rusql-executor/src/explain.rs`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Claiming optimizer cost parity
- Changing access-path selection (M49/M50)
- WAL

## Negative Constraints

- Do not emit MySQL 8.0 tree format unless tests pin it
- Do not implement EXPLAIN FOR CONNECTION

## Test plan

```bash
cargo test -p rusql-executor explain_json
cargo test -p rusql-server explain_json
```
