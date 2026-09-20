## Goal

Expose `mysql.user` columns connectors probe (plugin, host, ssl_type) without inventing hashes in SELECT.

## Category

Phase X — Security and TLS (M186). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

needs-human (auth catalog). Connectors SELECT mysql.user. Add missing columns as stubs or live fields matching M55 accounts. Never return password hashes in tests committed to git.


**Sequencing**: not `agent-ready`. Phase X / ADR-gated work stays `needs-human` until an ADR is accepted. Do not start while M114 file boundaries overlap.

## Acceptance Criteria

- [ ] `SELECT Host, User, plugin FROM mysql.user` returns catalog accounts (not errno 1146)
- [ ] Unknown extra columns: document or add empty stubs used by a listed connector probe
- [ ] Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-executor/src/info_schema.rs`
- `crates/rusql-core/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Writing authentication_string into mysql-diff golden files
- TLS implementation (M181)

## Negative Constraints

- Do not clone all 8.0 mysql.user columns if unused — add what probes need
- Do not grant world-readable password hashes

## Test plan

```bash
cargo test -p rusql-executor mysql_user_table
```
