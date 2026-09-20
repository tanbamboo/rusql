## Goal

Raise `SIGNAL SQLSTATE` / mysql_errno to the client; `RESIGNAL` from a handler later (M170) may no-op with docs if handlers missing.

## Category

Phase V — Stored programs (M166). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Apps and mysql-test sp* use SIGNAL. Map SQLSTATE + MESSAGE_TEXT + MYSQL_ERRNO to ERR packet. i18n for rusql-originated text; user MESSAGE_TEXT may pass through as SQL payload.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'nope', MYSQL_ERRNO = 1644` returns errno 1644 and message nope to the client
- [ ] Default SQLSTATE without MYSQL_ERRNO uses MySQL default mapping (pin 1644 or documented)
- [ ] RESIGNAL outside handler errors (errno 1645) until M170
- [ ] Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/stored_programs.rs`
- `crates/rusql-executor/src/programs.rs`
- `crates/rusql-protocol/src/**` (ERR packet fields if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Condition handlers (M170) except leaving hooks
- Changing SQLSTATE for existing executor errors

## Negative Constraints

- Do not implement GET DIAGNOSTICS
- Do not localize user MESSAGE_TEXT

## Test plan

```bash
cargo test -p rusql-executor signal
cargo test -p rusql-server signal
```
