## Goal

Write server errors/tracing to `--log-error` file.

## Category

Phase Y — Observability (M195). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Ops. Default stderr; flag writes file. Startup + panic-free error paths. i18n messages allowed in log as already formatted.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] With `--log-error=path`, a failed login or SQL error is appended
- [ ] Without flag, existing stderr/tracing unchanged
- [ ] Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-server/src/**`
- `crates/rusql-server/src/main.rs`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Changing default log level to debug in release
- Logging passwords

## Negative Constraints

- Do not implement log_error_verbosity full matrix
- Do not rotate logs in this slice

## Test plan

```bash
cargo test -p rusql-server log_error
```
