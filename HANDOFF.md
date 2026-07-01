# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-06-30 |
| Current issue | — |
| Branch | main |
| Next step | M18 GROUP BY (#41) — label `agent-ready` after M17 merge |
| Roadmap | [mysql-compat-roadmap.md](docs/en/specs/mysql-compat-roadmap.md) — issues #40–#58 |
| Book | #28 depth pass (Part 0, bibliography, M3/M4/M17 exemplars; roll out to remaining chapters) |

## Recent Progress

- M16 merged (#39): SELECT LIMIT (#38)
- M15 merged (#37): USE database (#36)
- M14 merged (#35): SELECT column projection (#34)
- Book merged (#33): mdBook en/zh-CN (#28)
- M13 merged (#32): SHOW CREATE TABLE (#31)
- M12 merged (#30): DESCRIBE / information_schema (#29)
- M11 merged (#27): COM_STMT_PREPARE / EXECUTE / CLOSE (#26)

## Ship checklist (every PR)

1. `CHANGELOG.md` → `[Unreleased]` then version section on release batch
2. `docs/en/release-notes.md` + zh-CN **Latest**
3. `user-guide.md` (en + zh) if user-testable
4. `node scripts/check-changelog.mjs`

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
node scripts/doc-parity.mjs
node scripts/check-changelog.mjs
```
