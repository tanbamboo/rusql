# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-02 |
| Branch | feat/m62-utf8mb4-0900-ai-ci |
| Next step | Merge PR [#160](https://github.com/tanbamboo/rusql/pull/160) (CI fix + dynamic mysql-diff port); then PR for M62 (#153) |

## Recent Progress

- **CI fix** PR [#160](https://github.com/tanbamboo/rusql/pull/160): `cargo fmt` (#158); mysql-diff port race fix (#159) — needs push of dynamic-port harness update
- **M62** (branch `feat/m62-utf8mb4-0900-ai-ci`): `utf8mb4_0900_ai_ci` collation + column `COLLATE` wiring (#153)
- **M63** ([#157](https://github.com/tanbamboo/rusql/pull/157) merged): CREATE FUNCTION scalar in SELECT
- **M64** ([#156](https://github.com/tanbamboo/rusql/pull/156) merged): AFTER UPDATE/DELETE triggers

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```
