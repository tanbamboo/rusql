# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-06-30 |
| Current issue | — |
| Branch | main |
| Next step | M14: `USE database` / multi-schema, or `SELECT` column projection |
| Book | [docs/book/README.md](docs/book/README.md) — living mdBook (#28) |

## Recent Progress

- Book MVP (#28): mdBook en/zh-CN under `docs/book/`, `check-book.mjs`
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
