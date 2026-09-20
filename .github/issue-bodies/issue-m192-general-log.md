## Goal

Optional general query log file (`--general-log-file`), off by default.

## Category

Phase Y — Observability (M192). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

After or independent of M191. Every COM_QUERY line. Must not log password hashes.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] When enabled, COM_QUERY text appears in the file; when disabled, no writes
- [ ] Connect events optional. Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-server/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Replacing tracing subscriber globally in a breaking way
- Audit JSON format (M185) unless shared

## Negative Constraints

- Do not enable in default CI (too noisy)
- Do not log resultsets

## Test plan

```bash
cargo test -p rusql-server general_log
```
