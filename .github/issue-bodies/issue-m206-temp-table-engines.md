## Goal

Honor `CREATE TEMPORARY TABLE … ENGINE=MEMORY|InnoDB` as documented rusql mappings (not silent ignore).

## Category

Phase Z — Remaining MySQL 8.0 surface (M206). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

CREATE TEMPORARY TABLE works (gap probe ok). ENGINE clause may be ignored. Map MEMORY to non-durable temp; InnoDB to default temp heap+WAL overlay. Reject unknown engines.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `ENGINE=MEMORY` temp table is session-scoped and gone after disconnect
- [ ] `ENGINE=InnoDB` accepted as rusql default engine; `ENGINE=foo` errors
- [ ] SHOW CREATE TABLE / SHOW TABLE STATUS engine cell documented
- [ ] Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-executor/src/**`
- `crates/rusql-storage/src/**`
- `crates/rusql-core/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- MyISAM implementation
- True InnoDB tablespaces (M198)

## Negative Constraints

- Do not persist MEMORY tables
- Do not change non-TEMPORARY ENGINE=InnoDB beyond existing default

## Test plan

```bash
cargo test -p rusql-executor temp_engine
cargo test -p rusql-server temp_engine
```
