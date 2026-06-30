# HANDOFF — Cross-Session State

> Update at the start and end of every agent session.

| Field | Value |
|-------|-------|
| Last updated | 2026-06-30 |
| Active profile | rust |
| Current issue | #2 PR #6 (needs-review); decisions from #3/#4 recorded |
| Branch | feature/m1-handshake |
| Blockers | None |
| Next step | Merge PR #6; create M2 SQL issue or caching_sha2 follow-up |

## Decisions Recorded (from Issue replies)

| Issue | Decision | ADR |
|-------|----------|-----|
| #3 | `mysql_native_password` MVP; `caching_sha2` follow-up | [adr-auth-mvp.md](docs/en/specs/adr-auth-mvp.md) |
| #4 | `sqlparser` MySQL dialect; targeted extensions later | [adr-sql-parser.md](docs/en/specs/adr-sql-parser.md) |

## Open Questions

- None blocking (awaiting PR #6 merge)

## Loop: Check Issue Replies

Every session start: `node scripts/check-issue-replies.mjs`

## Sensor Status

```
cargo fmt / clippy / test   OK
```
