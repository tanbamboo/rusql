## Goal

Serve official client `--ssl-mode=REQUIRED` using `--ssl-cert` / `--ssl-key` (ADR first).

## Category

Phase X — Security and TLS (M181). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Trust: must not autonomously change auth/TLS. needs-human until ADR. rustls or native-tls per ADR; no new crypto inventions.


**Sequencing**: not `agent-ready`. Phase X / ADR-gated work stays `needs-human` until an ADR is accepted. Do not start while M114 file boundaries overlap.

## Acceptance Criteria

- [ ] ADR for TLS stack and CLI flags
- [ ] After ADR: mysql client `--ssl-mode=REQUIRED` connects; without TLS server, REQUIRED fails
- [ ] Non-TLS default still works for local tests
- [ ] Docs as usual; no secrets in repo

## File Boundaries

Allowed:
- `docs/en/architecture/**`, `docs/zh-CN/architecture/**`
- `crates/rusql-server/src/**`
- `crates/rusql-protocol/src/**`
- `crates/rusql-i18n/**`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- CONSTITUTION.md
- Committing cert private keys
- Disabling auth to make TLS tests pass

## Negative Constraints

- Do not implement client-cert REQUIRE X509 in this slice (M184)
- Do not add enterprise plugins

## Test plan

```bash
cargo test -p rusql-server tls_listen
# plus documented mysql --ssl-mode=REQUIRED manual/CI step
```
