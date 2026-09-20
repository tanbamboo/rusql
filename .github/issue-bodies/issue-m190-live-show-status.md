## Goal

Replace cheap SHOW STATUS constants with live `Questions`, `Uptime`, `Threads_connected`.

## Category

Phase Y — Observability (M190). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M86 SHOW STATUS stubs. Ops and connectors read Questions/Uptime. Increment Questions per COM_QUERY; Uptime from process start; Threads from connection count.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] Two COM_QUERY `SHOW STATUS LIKE 'Questions'` — second Questions >= first + 1
- [ ] Uptime increases after sleep in test (or monotonic mock clock)
- [ ] Other STATUS names stay stubs. Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-executor/src/show_status.rs`
- `crates/rusql-server/src/**`
- `crates/rusql-core/src/processlist.rs`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Full InnoDB status (M193)
- performance_schema (M189) except reuse counters

## Negative Constraints

- Do not implement SHOW STATUS LIKE as regex beyond existing LIKE
- Do not add 400+ MySQL status names in one PR — live three + keep stubs

## Test plan

```bash
cargo test -p rusql-executor show_status_live
cargo test -p rusql-server show_status_live
```
