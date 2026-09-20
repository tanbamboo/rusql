## Goal

Privilege via a role graph: `CREATE ROLE`, `GRANT role TO user`, `SET ROLE`.

## Category

Phase X — Security and TLS (M182). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

needs-human (authz model). M54 GRANT/REVOKE exists for privileges. Roles are named privilege bundles. ADR if graph changes mysql.user.json.


**Sequencing**: not `agent-ready`. Phase X / ADR-gated work stays `needs-human` until an ADR is accepted. Do not start while M114 file boundaries overlap.

## Acceptance Criteria

- [ ] `CREATE ROLE r`; `GRANT SELECT ON db.* TO r`; `GRANT r TO u`; `SET ROLE r` allows SELECT for u
- [ ] REVOKE role removes access. SHOW GRANTS includes role (documented format)
- [ ] Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/grants.rs`
- `crates/rusql-core/src/privileges.rs`
- `crates/rusql-executor/src/privileges.rs`
- `docs/en/architecture/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- TLS (M181) in the same PR unless already merged
- Default role graph cycles without detection

## Negative Constraints

- Do not implement mandatory roles
- Do not add enterprise audit in this slice

## Test plan

```bash
cargo test -p rusql-executor roles
cargo test -p rusql-sql create_role
```
