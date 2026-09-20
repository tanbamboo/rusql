## Goal

Procedure-local `DECLARE v INT DEFAULT 0` visible in the BEGIN…END body.

## Category

Phase V — Stored programs (M167). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

P3 procedures execute statements without local vars. DECLARE must precede handlers/cursors per MySQL order — this slice: variables only.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `DECLARE n INT DEFAULT 1; SET n = n + 1; SELECT n` inside a proc returns `2`
- [ ] Unknown local var vs user-var: bare `n` is local; `@n` is user
- [ ] DECLARE after statements errors (MySQL 1337 / documented)
- [ ] Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/stored_programs.rs`
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
- IF/WHILE (M168) except SET/SELECT already working
- programs.json breaking changes

## Negative Constraints

- Do not implement DECLARE … TYPE with nested types
- Do not add session variables for DECLARE

## Test plan

```bash
cargo test -p rusql-executor declare_var
cargo test -p rusql-sql declare_var
```
