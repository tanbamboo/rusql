## Goal

`FULLTEXT` index + `MATCH … AGAINST` boolean/natural mode subset.

## Category

Phase Z — Remaining MySQL 8.0 surface (M197). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

InnoDB FULLTEXT is large; MVP: inverted index on VARCHAR/TEXT for BOOLEAN MODE `+word`. Natural language can be later.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `CREATE FULLTEXT INDEX` + `MATCH(col) AGAINST ('+foo' IN BOOLEAN MODE)` returns rows containing foo
- [ ] No index: MATCH errors or documented full scan — pin one
- [ ] Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-storage/src/**`
- `crates/rusql-executor/src/**`
- `crates/rusql-planner/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Parser ngram Chinese as a silent extra without tests
- GIS (M196)

## Negative Constraints

- Do not claim InnoDB ranking identical to MySQL
- Do not add query expansion

## Test plan

```bash
cargo test -p rusql-storage fulltext
cargo test -p rusql-executor match_against
```
