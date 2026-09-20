## Goal

Honor `REQUIRE SSL` on an account (depends on M181).

## Category

Phase X — Security and TLS (M184). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

needs-human. After TLS listener: accounts with REQUIRE SSL must not query on plaintext connections (errno 1045/3159 documented).


**Sequencing**: not `agent-ready`. Phase X / ADR-gated work stays `needs-human` until an ADR is accepted. Do not start while M114 file boundaries overlap.

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
