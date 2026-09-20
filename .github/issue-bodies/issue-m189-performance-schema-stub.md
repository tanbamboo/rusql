## Goal

`performance_schema` exists (not errno 1146) with documented stub tables (`accounts`, `events_statements_digest` or equivalent subset).

## Category

Phase Y — Observability (M189). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Connectors probe performance_schema. Stub empty/counter tables. Do not fake InnoDB mutex data (M193).


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `SELECT * FROM performance_schema.session_variables` or documented first tables succeed (0+ rows, not 1146)
- [ ] List of implemented tables in user-guide; others 1146 or empty documented
- [ ] Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-executor/src/info_schema.rs`
- `crates/rusql-sql/src/**`
- `crates/rusql-core/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Claiming full P_S instrument parity
- INNODB_* (M194) except if already stubbed

## Negative Constraints

- Do not enable all instruments by default (CPU)
- Do not persist digest table to WAL

## Test plan

```bash
cargo test -p rusql-executor performance_schema
cargo test -p rusql-server performance_schema
```
