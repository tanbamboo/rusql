## Goal

`FLUSH PRIVILEGES` reloads `mysql.user.json` (or equivalent) into the live grant table.

## Category

Phase X — Security and TLS (M187). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

ADR accepted: [adr-m187-flush-privileges.md](../../docs/en/architecture/adr-m187-flush-privileges.md). Reload holds the privilege catalog write lock, so authentication sees either the previous catalog or the fully parsed file. After a file edit or GRANT, FLUSH reloads. A GRANT that is already live still returns OK and re-reads disk.

## Acceptance Criteria

- [x] `FLUSH PRIVILEGES` returns OK. After replacing catalog file (test), new accounts can login
- [x] `FLUSH TABLES` / other FLUSH variants: implement privileges only; others may error documented
- [x] Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-executor/src/privileges.rs`
- `crates/rusql-server/src/**`
- `crates/rusql-core/src/privileges.rs`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- FLUSH TABLES WITH READ LOCK (M163 extra)
- Dropping in-memory databases

## Negative Constraints

- Do not implement FLUSH HOSTS unless one test needs it
- Do not reload TLS certs here (M181)

## Test plan

```bash
cargo test -p rusql-executor flush_privileges
cargo test -p rusql-server flush_privileges
```
