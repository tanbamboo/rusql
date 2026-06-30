# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-06-30 |
| Current issue | #16 M6 — PR pending |
| Branch | feature/m6-auth-and-dml |
| Next step | Merge PR; M7 caching_sha2 (#7) or replication ADR (#5) |

## Recent Progress

- M6: `mysql_native_password` verify (`--auth-password`), DROP TABLE, DELETE
- ADR: [docs/en/specs/adr-m6-auth-and-dml.md](docs/en/specs/adr-m6-auth-and-dml.md)

## Loop

Session start: `node scripts/check-issue-replies.mjs`

## Sensors

All green locally (fmt, clippy, test, harness-validate)
