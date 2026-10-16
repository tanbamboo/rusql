# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-16 |
| Branch | `issue-300-m148-enum-set` — M148 ENUM/SET PR pending merge ([#300](https://github.com/tanbamboo/rusql/issues/300)) |
| Next step | After M148 merge: pick **M149 DEFAULT expressions** ([#301](https://github.com/tanbamboo/rusql/issues/301)) (`agent-ready`, next Phase T slice). Ultimate MySQL 8.0 goal is **not** complete (Phases T–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-16)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M147 merge; M148 PR in flight |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phases T–Z (M146–M210) | **In progress** — M146–M147 on `main`; M148 in PR #300 branch. Next sequenced after merge: [M149 #301](https://github.com/tanbamboo/rusql/issues/301). Milestones [T](https://github.com/tanbamboo/rusql/milestone/11)–[Z](https://github.com/tanbamboo/rusql/milestone/17); issues #298–#363 |

| Estimated surface | ~45–70% client-visible; remaining work is Phase T through Z (including M209/M210) |

## Recent Progress

- **M148 (in PR)** — ENUM/SET types: member validation, SET canonical order, errno 1265 strict, SHOW CREATE, mysql-diff `enum_set_types` (#300)
- **M147 merged** — CHECK constraints: catalog/WAL `CheckConstraintMeta`, INSERT/UPDATE errno 3819, SHOW CREATE TABLE (#299 / PR #397)
- **M146 merged** — VIRTUAL generated columns (#298 / PR #396)

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
