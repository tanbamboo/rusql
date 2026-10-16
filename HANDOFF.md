# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-16 |
| Branch | `issue-299-m147-check-constraints` — M147 PR pending merge (#299) |
| Next step | After M147 merge: pick **M148 ENUM/SET** ([#300](https://github.com/tanbamboo/rusql/issues/300)) (next Phase T slice). Phase S **complete** on `main`. M146 merged. Ultimate MySQL 8.0 goal is **not** complete (Phases T–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-16)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M146 merge |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phases T–Z (M146–M210) | **In progress** — M147 in PR #299; next sequenced after merge: [M148 #300](https://github.com/tanbamboo/rusql/issues/300) |

| Estimated surface | ~45–70% client-visible; remaining work is Phase T through Z (including M209/M210) |

## Recent Progress

- **M147 in flight** — CHECK constraints: catalog/WAL `CheckConstraintMeta`, INSERT/UPDATE enforce errno 3819, SHOW CREATE TABLE (#299)
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
