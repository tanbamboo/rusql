# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-06-30 |
| Current issue | — |
| Branch | main |
| Next step | M11: COM_STMT_PREPARE |

## Recent Progress

- #23 merged (#25): CHANGELOG + release notes + check-changelog sensor
- M10 merged (#25): SHOW TABLES / SHOW DATABASES (#24)

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
node scripts/doc-parity.mjs
node scripts/check-changelog.mjs
```
