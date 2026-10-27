## Goal

Expose `mysql.user` columns connectors probe (plugin, host, ssl_type) without inventing hashes in SELECT.

## Category

Phase X — Security and TLS (M186). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

ADR accepted (adr-m186-mysql-user.md). Virtual `SELECT mysql.user`: `Host`, `User`, `plugin`, `ssl_type`. `authentication_string` is errno 1054 (no hash in the result). Unauthorized SELECT is errno 1142. Other `mysql.*` tables stay errno 1146.


**Sequencing**: ADR accepted in `docs/en/architecture/adr-m186-mysql-user.md`. M187 stays `needs-human` until its own ADR.

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
