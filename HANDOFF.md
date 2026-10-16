# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-16 |
| Branch | `main` (M147 merged — PR [#397](https://github.com/tanbamboo/rusql/pull/397) / #299) |
| Next step | Pick **M148 ENUM/SET types** ([#300](https://github.com/tanbamboo/rusql/issues/300)) (`agent-ready`, next Phase T slice). Phase S **complete** on `main`. Ultimate MySQL 8.0 goal is **not** complete (Phases T–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-16)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M147 merge (mysql-diff suite `check_constraint`) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phases T–Z (M146–M210) | **In progress** — M146–M147 merged; next sequenced: [M148 #300](https://github.com/tanbamboo/rusql/issues/300) (`agent-ready`). Milestones [T](https://github.com/tanbamboo/rusql/milestone/11)–[Z](https://github.com/tanbamboo/rusql/milestone/17); issues #298–#363 |

| Estimated surface | ~45–70% client-visible; remaining work is Phase T through Z (including M209/M210) |

## Recent Progress

- **M147 merged** — CHECK constraints: catalog/WAL `CheckConstraintMeta`, INSERT/UPDATE errno 3819, SHOW CREATE TABLE (#299 / PR #397)
- **M146 merged** — VIRTUAL generated columns (#298 / PR #396)
- **M145 merged** — EXPLAIN FORMAT=JSON stub (#297 / PR #395)

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
