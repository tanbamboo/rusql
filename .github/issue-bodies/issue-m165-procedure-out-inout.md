## Goal

`CALL` assigns `OUT`/`INOUT` parameters to user variables.

## Category

Phase V — Stored programs (M165). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M132 IN params. MySQL `CALL p(@x)` with `OUT x`. User variables M82. Additive program catalog fields.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `CREATE PROCEDURE p(OUT x INT) BEGIN SET x = 3; END` then `CALL p(@a)`; `SELECT @a` → `3`
- [ ] `INOUT` reads the incoming user-var value and writes back
- [ ] CALL without user-var for OUT errors (MySQL 1414 / documented). IN-only procs unchanged
- [ ] SHOW CREATE PROCEDURE includes OUT/INOUT. Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/stored_programs.rs`
- `crates/rusql-core/src/programs.rs`
- `crates/rusql-executor/src/programs.rs`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Cursors (M169)
- SIGNAL (M166)
- Breaking programs.json (additive serde default)

## Negative Constraints

- Do not implement parameter INOUT with literals
- Do not add FUNCTIONS OUT params (functions stay RETURN)

## Test plan

```bash
cargo test -p rusql-executor procedure_out
cargo test -p rusql-sql procedure_out
```
