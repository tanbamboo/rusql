## Goal

Documented subset of password expiration / policy for caching_sha2 accounts.

## Category

Phase X — Security and TLS (M183). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

ADR accepted: [adr-m183-password-policy.md](../../docs/en/architecture/adr-m183-password-policy.md). M55/M7 hashing stays. This slice enforces `PASSWORD EXPIRE` (errno 1820), password reuse (errno 3638), and built-in `validate_password` (errno 1819, off by default).


**Sequencing**: not `agent-ready`. Phase X / ADR-gated work stays `needs-human` until an ADR is accepted. Do not start while M114 file boundaries overlap.

## Acceptance Criteria

- [x] Either: expired user cannot COM_QUERY until ALTER USER password, errno 1820, or rusql-vs-mysql lists skip with SKIPS
- [x] Non-expired caching_sha2 login unchanged
- [x] Docs as usual; no plaintext passwords in tests beyond fixtures

## File Boundaries

Allowed:
- `crates/rusql-protocol/src/auth/**`
- `crates/rusql-server/src/**`
- `crates/rusql-sql/src/**` (ALTER USER)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Inventing a password-validate plugin .so (M201)
- Weakening hashing

## Negative Constraints

- Do not implement password history table unless catalog reuse is obvious
- Do not add complexity checks beyond documented subset

## Test plan

```bash
cargo test -p rusql-protocol password_expire
cargo test -p rusql-server password_expire
```
