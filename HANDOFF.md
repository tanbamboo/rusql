# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-06-30 |
| Current issue | M19 (#42) shipping |
| Branch | feature/m19-offset |
| Next step | M21 IS NULL (#44) — label `agent-ready` after M20 merge |
| Roadmap | [mysql-compat-roadmap.md](docs/en/specs/mysql-compat-roadmap.md) |
| Book | #28 depth pass (M0–M13 remaining) |

## Recent Progress

- M18 merged (#61): column aliases (#41)
- M17 merged (#59): ORDER BY (#40)
- Book depth #60, roadmap #59

## Ship checklist (every PR)

1. `CHANGELOG.md` → `[Unreleased]`
2. `docs/en/release-notes.md` + zh-CN **Latest**
3. `user-guide.md` (en + zh) if user-testable
4. `node scripts/check-changelog.mjs`

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
```
