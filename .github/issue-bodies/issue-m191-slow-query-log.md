## Goal

Threshold + file for slow queries (`long_query_time`, `--slow-query-log-file`).

## Category

Phase Y — Observability (M191). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Ops feature. Off by default. Log SQL exceeding threshold. Format can be a documented subset of MySQL slow log.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] With log enabled and long_query_time=0, a SELECT is appended to the file
- [ ] Disabled: no file writes. SET SESSION long_query_time works or documented global-only
- [ ] Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-server/src/**`
- `crates/rusql-executor/src/session_var.rs`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- General log (M192) in the same PR unless trivial shared writer
- P_S digest

## Negative Constraints

- Do not log passwords in COM_CHANGE_USER
- Do not implement microsecond slow log extra columns unless cheap

## Test plan

```bash
cargo test -p rusql-server slow_query_log
```
