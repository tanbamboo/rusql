# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-21 |
| Branch | `issue-328-m176-gtid-executed` (M176 ready for PR) |
| Next step | **Ship M176** ([#328](https://github.com/tanbamboo/rusql/issues/328)) — open PR / merge when CI green. Then label M179 `agent-ready`; Phase X needs human ADR. Ultimate goal **not** complete until Phase Z M209/M210. |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence on `main`.

## Status vs goal (2026-10-21)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M175 merge (`6e2ce5c`) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phase T (M146–M156) | **Complete** on `main` — Phase T exit (simple mysqldump load) still open |
| Phase U (M157–M164) | **Complete** on `main` |
| Phase V (M165–M172) | **Complete** on `main` |
| Phase W M173–M175 | **Complete** on `main` |
| Phase W M176 | **Ready for PR** on `issue-328-m176-gtid-executed` |
| Phase W M177–M180 | Open (M177 `needs-human`) |
| Phase X–Z | Open; X mostly `needs-human` until ADR |

| Estimated surface | ~45–70% client-visible; remaining = Phase W remainder + X + Y + Z (incl. M209/M210) |

### Milestone backlog (open issues)

| Phase | Milestone | Open | Closed | Blockers |
|-------|-----------|------|--------|----------|
| W | Replication production | 6 | 2 | Ship M176 → M179/M180; M177 human |
| X | Security and TLS | 8 | 0 | ADR before `agent-ready` |
| Y | Observability | 7 | 0 | After W / parallel where safe |
| Z | Remaining + DoD | 15 | 0 | M209/M210 = ultimate exit |

## Complete approach plan (constitution-aligned)

Principles: **spec-first**, one issue / one PR / `area:*`, file boundaries, sensors before done, i18n, no autonomous T0/auth-TLS without ADR, `agent-ready` only when deps on `main`.

### Stage 1 — Finish Phase W (replication production)

| Order | ID | Issue | Priority | Notes |
|-------|----|-------|----------|-------|
| 1 | M176 | [#328](https://github.com/tanbamboo/rusql/issues/328) | P1 | `@@gtid_executed` persistence — **ready for PR** |
| 2 | M179 | [#331](https://github.com/tanbamboo/rusql/issues/331) | P2 | `CHANGE MASTER TO` / `START SLAVE` |
| 3 | M180 | [#332](https://github.com/tanbamboo/rusql/issues/332) | P3 | Row event v2 / partial images |
| 4 | M178 | [#330](https://github.com/tanbamboo/rusql/issues/330) | P3 | Semi-sync ACK stub or reject |
| 5 | M177 | [#329](https://github.com/tanbamboo/rusql/issues/329) | P2 | Failover — **needs-human** ADR |

**Exit**: Primary → replica row consistency + GTID failover doc.

### Stage 2 — Phase Y observability (parallel-safe after W core)

| Order | ID | Issue | Priority |
|-------|----|-------|----------|
| 1 | M190 | [#342](https://github.com/tanbamboo/rusql/issues/342) | P1 Live SHOW STATUS |
| 2 | M189 | [#341](https://github.com/tanbamboo/rusql/issues/341) | P2 performance_schema stub |
| 3 | M191 | [#343](https://github.com/tanbamboo/rusql/issues/343) | P2 Slow query log |
| 4 | M195 | [#348](https://github.com/tanbamboo/rusql/issues/348) | P2 Error log |
| 5 | M192–M194 | [#344](https://github.com/tanbamboo/rusql/issues/344)–[#347](https://github.com/tanbamboo/rusql/issues/347) | P3 |

### Stage 3 — Phase X security (human ADR gate)

Do **not** `agent-ready` until ADR accepted. Sequence after ADR: M181 TLS → M184 REQUIRE SSL → M186/M187 → M182 roles → M183/M185/M188 docs.

### Stage 4 — Phase Z remaining surface + definition of done

| Order | ID | Issue | Priority | Notes |
|-------|----|-------|----------|-------|
| … | M203–M206 | [#356](https://github.com/tanbamboo/rusql/issues/356)–[#359](https://github.com/tanbamboo/rusql/issues/359) | P2 | Optimizer / temp tables |
| … | M197 | [#350](https://github.com/tanbamboo/rusql/issues/350) | P3 | FULLTEXT |
| … | M196, M198–M202 | various | P2–P3 | GIS / GR / plugins — ADR or document skip |
| late | M207 | [#360](https://github.com/tanbamboo/rusql/issues/360) | P1 | mysql-test 100→500 |
| late | M208 | [#361](https://github.com/tanbamboo/rusql/issues/361) | P1 | Gap probe CI floor |
| **DoD** | M209 | [#362](https://github.com/tanbamboo/rusql/issues/362) | P0 | Requirement matrix |
| **DoD** | M210 | [#363](https://github.com/tanbamboo/rusql/issues/363) | P0 | Production drop-in gate |

**Ultimate exit** (all proven): matrix has no unexpected Missing; mysql-diff + gap probe + mysql-test + locking/replication green vs `mysql:8.0`; enterprise skips explicit; HANDOFF may say achieved only then.

### Agent loop

1. `node scripts/check-issue-replies.mjs`
2. Pick highest `priority` among `agent-ready` (skip `needs-human`)
3. Spec → implement (subagent) → sensors → docs → PR → merge
4. Label next issue `agent-ready`; update this file; repeat until M209+M210 proven

## Recent Progress

- **M175 merged** — Live `SHOW MASTER STATUS` / `SHOW BINARY LOG STATUS` (#327 / PR #428)
- **M174 merged** — Idle dump follow sends `HEARTBEAT_LOG_EVENT` (27); applier ignores (#326 / PR #427)
- **M173 merged** — `GTID_LOG_EVENT` (33) before row events (#325 / PR #426)
- **M176 ready for PR** — `@@gtid_executed` persistence, `Executed_Gtid_Set`, docs/tests on branch (not merged)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M176: empty `@@gtid_executed` on fresh server; set persists in `{data_dir}/gtid.json`; `SHOW MASTER STATUS`.`Executed_Gtid_Set` matches. Not `gtid_mode` UI / GR / `mysql.gtid_executed`.
- M175 gap: `SHOW BINARY LOG STATUS` CI MySQL 8.0 image lacks 8.4 synonym.
- Phase W exit not complete until M176 merge + M179/M180 (+ M177 ADR).
- 15 issues still `needs-human` (Phase X + some Z) — do not pick until human replies / ADR.
