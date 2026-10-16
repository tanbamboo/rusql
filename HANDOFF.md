# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-16 |
| Branch | `main` (after M151 merge — PR for #303) |
| Next step | **M152 Table partitioning MVP (RANGE)** ([#304](https://github.com/tanbamboo/rusql/issues/304)) (`agent-ready`, next Phase T slice). Ultimate MySQL 8.0 goal is **not** complete (Phases T–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-16)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M151 merge (mysql-diff suite `functional_index`) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phases T–Z (M146–M210) | **In progress** — M146–M151 merged; next sequenced: [M152 #304](https://github.com/tanbamboo/rusql/issues/304) (`agent-ready`). Milestones [T](https://github.com/tanbamboo/rusql/milestone/11)–[Z](https://github.com/tanbamboo/rusql/milestone/17); issues #298–#363 |

| Estimated surface | ~45–70% client-visible; remaining work is Phase T through Z (including M209/M210) |

## Recent Progress

- **M151 merged** — Functional indexes `INDEX ((expr))`; equality lookup + EXPLAIN ref; mysql-diff `functional_index` (#303)
- **M150 merged** — `INVISIBLE` columns/indexes; mysql-diff `invisible_column` (#302)
- **M149 merged** — DEFAULT expressions; mysql-diff `default_expr` (#301)
- **M148 merged** — ENUM/SET types (#300 / PR #398)
- **M147 merged** — CHECK constraints (#299 / PR #397)
- **M146 merged** — VIRTUAL generated columns (#298 / PR #396)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M151 MVP: one arithmetic expression per index; unknown functions error at DDL (i18n). Not descending/prefix functional indexes.
- Phase T exit (mysqldump simple schema) still open after M152+ slices.
