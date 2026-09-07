## Goal

Implement MySQL session/info built-ins that clients and ORMs call constantly: `DATABASE()`, `SCHEMA()`, `USER()`, `CURRENT_USER()`, `VERSION()`.

## Background

Post M0–M64 / PERF-B*, the M36–M61 roadmap is complete and CI is green. Remaining client-visible gaps still block ORM/admin tooling. During #159 investigation, `SELECT DATABASE()` failed with `unsupported function: DATABASE`. MySQL CLI and drivers rely on these functions for connection setup and diagnostics.

## Category

SQL expressions / session metadata (Phase Q — post-parity extensions).

## Acceptance Criteria

- [ ] `SELECT DATABASE()` / `SELECT SCHEMA()` return the current session database (default `rusql`; after `USE app_db`, return `app_db`)
- [ ] `SELECT USER()` and `SELECT CURRENT_USER()` return `user@host` (host from session; `%` acceptable for MVP)
- [ ] `SELECT VERSION()` returns a MySQL 8.0-compatible string containing `8.0` (e.g. `8.0.33-rusql`)
- [ ] Functions work in `SELECT` without `FROM` and as projection expressions
- [ ] Wire/integration test covers DATABASE after USE, USER/CURRENT_USER, VERSION
- [ ] `mysql-diff` suite (or extended expressions suite) compares these against Docker MySQL 8.0 where portable
- [ ] Docs: CHANGELOG, release-notes (en + zh-CN), user-guide (en + zh-CN)

## File Boundaries

Allowed:
- `crates/rusql-executor/src/expr.rs`
- `crates/rusql-executor/src/lib.rs` (only if needed for dual-path SELECT)
- `crates/rusql-server/src/**` (tests)
- `crates/rusql-server/compat/mysql-diff.json`
- `tests/mysql-test/**` (optional portable cases)
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `docs/en/specs/mysql-full-parity-roadmap.md` (+ zh-CN mirror) — add Phase Q M65 row

Forbidden:
- Storage engine redesign
- Auth plugin changes
- Replication rewrite

## Negative Constraints

- Do not implement full `SHOW VARIABLES` / all `@@` system variables in this milestone
- Do not change handshake version negotiation semantics beyond what VERSION() returns
- No new crates / external deps

## Test plan

```bash
cargo test -p rusql-executor
cargo test -p rusql-server session_info
node scripts/mysql-diff.mjs   # new/extended suite green
```
