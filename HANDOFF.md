# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-16 |
| Branch | `main` (M146 merged — PR [#396](https://github.com/tanbamboo/rusql/pull/396) / #298) |
| Next step | Pick **M147 CHECK constraints** ([#299](https://github.com/tanbamboo/rusql/issues/299)) (`agent-ready`, next Phase T slice). Phase S **complete** on `main` (M133–M145). Ultimate MySQL 8.0 goal is **not** complete (Phases T–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-16)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M146 merge (mysql-diff suite `generated_column_virtual`) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** — last merge M113 PR #263 |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` — last merge M145 PR #395 |
| Phases T–Z (M146–M210) | **In progress** — M146 merged; next sequenced: [M147 #299](https://github.com/tanbamboo/rusql/issues/299) (`agent-ready`). Milestones [T](https://github.com/tanbamboo/rusql/milestone/11)–[Z](https://github.com/tanbamboo/rusql/milestone/17); issues #298–#363 |

| Estimated surface | ~45–70% client-visible; remaining work is Phase T through Z (including M209/M210) |

## Recent Progress

- **M146 merged** — VIRTUAL generated columns `AS (expr)`: catalog/WAL `VirtualGeneratedColumn`, SELECT-time eval, errno 3105 on INSERT/UPDATE of generated col, STORED rejected, mysql-diff `generated_column_virtual` (#298 / PR #396)
- **M145 merged** — `EXPLAIN FORMAT=JSON SELECT …` documented JSON stub (#297 / PR #395)
- **M144 merged** — Two-table `INNER JOIN` `UPDATE`/`DELETE` (#296 / PR #394)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
node scripts/mysql-gap-probe.mjs   # inventory only; not a CI gate
```
