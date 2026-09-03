# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-03 |
| Branch | feat/m62-utf8mb4-0900-ai-ci |
| Next step | Land PR [#161](https://github.com/tanbamboo/rusql/pull/161) (M62 #153) after CI green |

## Recent Progress

- **#160 merged** — CI green: fmt (#158); mysql-diff USE via `-D` + status-only compare + spawn_blocking oracle tests (#159)
- **#159 root cause**: Server was **not** crashing. Official MySQL 8.0 CLI rejects `-e "USE db"` on a fresh TCP connection; use `-D db` (handshake COM_INIT_DB).
- **M62** (branch `feat/m62-utf8mb4-0900-ai-ci`): `utf8mb4_0900_ai_ci` collation + column `COLLATE` wiring (#153) — PR [#161](https://github.com/tanbamboo/rusql/pull/161)
- Subagent deliverables merged: PERF-B2/B3 ([#147](https://github.com/tanbamboo/rusql/pull/147)), M51–M53 ([#148](https://github.com/tanbamboo/rusql/pull/148))
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
