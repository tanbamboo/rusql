# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-17 |
| Branch | `main` (after M171 merge — PR TBD for #323) |
| Next step | **M172 DELIMITER in COM_QUERY** ([#324](https://github.com/tanbamboo/rusql/issues/324)) — label `agent-ready` after M171 merge. Ultimate MySQL 8.0 goal is **not** complete (Phases V–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-17)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M171 merge (pending) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phase T (M146–M156) | **Complete** on `main` — Phase T exit (simple mysqldump load) still open |
| Phase U (M157–M164) | **Complete** on `main` |
| Phase V (M165–M172) | **M165–M171** on branch/merge; **M172 #324** next |

| Estimated surface | ~45–70% client-visible; remaining work is Phase V through Z (including M209/M210) |

## Recent Progress

- **M171 (in flight)** — All six row trigger timings on DML; `BEFORE` `SIGNAL` abort; `AFTER INSERT`; `DROP TRIGGER IF EXISTS` parse fix; mysql-diff `trigger_timings` (#323)
- **M170 merged** — `DECLARE CONTINUE|EXIT HANDLER FOR` NOT FOUND / SQLEXCEPTION / mysql errno; cursor **1329** catchable; EXIT leaves block without aborting handled `CALL`; `RESIGNAL` in handler (#322 / PR #423)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M171 gaps (documented): no statement-level triggers; `BEFORE` bodies limited to `SET NEW` / `SIGNAL` (not full arbitrary DML like `AFTER`); `AFTER DELETE` still fires before physical delete (pre-existing); no `FOLLOWS` / multiple triggers per timing.
- M172 next: `DELIMITER` in COM_QUERY or documented mysql CLI limitation.
