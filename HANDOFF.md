# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-16 |
| Branch | `issue-304-m152-range-partition` (PR for #304 M152 RANGE partitioning) |
| Next step | **M153 ALTER TABLE ADD/DROP INDEX (dump forms)** ([#305](https://github.com/tanbamboo/rusql/issues/305)) after M152 merge. Ultimate MySQL 8.0 goal is **not** complete (Phases T–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-16)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M151 merge; M152 PR pending |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phases T–Z (M146–M210) | **In progress** — M146–M151 on `main`; M152 in PR [#304](https://github.com/tanbamboo/rusql/issues/304); next sequenced: [M153 #305](https://github.com/tanbamboo/rusql/issues/305) |

| Estimated surface | ~45–70% client-visible; remaining work is Phase T through Z (including M209/M210) |

## Recent Progress

- **M152 (in PR)** — RANGE `PARTITION BY` catalog + per-partition heaps; INSERT route; SELECT/EXPLAIN equality prune; ADR + mysql-diff `partition_range`
- **M151 merged** — Functional indexes `INDEX ((expr))`; mysql-diff `functional_index` (#303)
- **M150 merged** — `INVISIBLE` columns/indexes (#302)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M152 MVP: INT `VALUES LESS THAN` / `MAXVALUE`; no HASH/LIST/KEY, subpartition, or exchange.
- EXPLAIN text adds `Extra` column (partition prune in `partitions: …`).
- Phase T exit (mysqldump simple schema) still open after M153+ slices.
