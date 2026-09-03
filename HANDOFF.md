# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-02 |
| Branch | fix/ci-main |
| Next step | Land final #159 harness tweak (USE status-only compare); merge [#160](https://github.com/tanbamboo/rusql/pull/160); then M62 [#153](https://github.com/tanbamboo/rusql/issues/153) via [#161](https://github.com/tanbamboo/rusql/pull/161) |

## Recent Progress

- **#159 root cause**: Server was **not** crashing. Official MySQL 8.0 CLI rejects `-e "USE db"` on a fresh TCP connection (`Can't connect to the server`); use `-D db` (handshake COM_INIT_DB). Embedded oracle tests must use `spawn_blocking` for `mysql` subprocesses so the in-process TestServer keeps accepting connections.
- **CI fix** PR [#160](https://github.com/tanbamboo/rusql/pull/160): fmt (#158); mysql-diff USE via `-D` + oracle test fixes (#159)
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
