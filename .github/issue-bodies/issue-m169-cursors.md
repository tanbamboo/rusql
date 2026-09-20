## Goal

One open cursor per CALL: `DECLARE CURSOR`, `OPEN`, `FETCH`, `CLOSE`.

## Category

Phase V — Stored programs (M169). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

After M167. NOT FOUND handler may wait for M170 — FETCH past end can error 1328 or set dummy until handlers exist (document).


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] Cursor over `SELECT id FROM t ORDER BY id` FETCHes rows in order into local vars
- [ ] FETCH after last row: errno 1328 or handler hook documented
- [ ] Only one open cursor required. Nested cursors out of scope
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
- Updatable cursors / WHERE CURRENT OF
- Handlers (M170) except NOT FOUND stub
- Server-wide cursor state across CALLs

## Negative Constraints

- Do not implement INSENSITIVE vs SENSITIVE distinction
- Do not add cursor WITH RETURN

## Test plan

```bash
cargo test -p rusql-executor cursor_fetch
cargo test -p rusql-sql cursor_decl
```
