## Goal

Honor `REQUIRE SSL` on an account (depends on M181).

## Category

Phase X — Security and TLS (M184). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

ADR accepted ([adr-m184-require-ssl.md](../../docs/en/architecture/adr-m184-require-ssl.md)). Plaintext login for a `REQUIRE SSL` account is errno **1045**. Errno **3159** stays the server-wide `--require-secure-transport` flag (M181). `REQUIRE X509` / `ISSUER` / `SUBJECT` / `CIPHER` are errno **1064**.

## Acceptance Criteria

- [ ] User WITH REQUIRE SSL fails on non-TLS; succeeds on TLS
- [ ] Users without REQUIRE SSL still work on plaintext
- [ ] SHOW CREATE USER includes REQUIRE SSL. Tests + docs

## File Boundaries

Allowed:
- `crates/rusql-server/src/**`
- `crates/rusql-protocol/src/**`
- `crates/rusql-core/src/**` (account flags)
- `crates/rusql-executor/src/show_create_user.rs`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- REQUIRE X509 / cipher lists except document skip
- Disabling TLS tests in CI to go green

## Negative Constraints

- Do not implement REQUIRE ISSUER
- Do not store certs in the repo

## Test plan

```bash
cargo test -p rusql-server require_ssl
```
