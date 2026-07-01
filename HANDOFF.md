# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-06-30 |
| Current issue | M18 (#41) shipping |
| Branch | feature/m18-column-aliases |
| Next step | M19 OFFSET (#42) — label `agent-ready` after M18 merge |
| Roadmap | [mysql-compat-roadmap.md](docs/en/specs/mysql-compat-roadmap.md) — issues #40–#58 |
| Book | #28 depth pass (M0–M13 remaining) |

## Recent Progress

- M17 merged (#59): ORDER BY (#40)
- Roadmap + book depth (#59), book pass 2 (#60)
- M16 merged (#39): SELECT LIMIT (#38)
- M15 merged (#37): USE database (#36)
- M14 merged (#35): SELECT column projection (#34)

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
