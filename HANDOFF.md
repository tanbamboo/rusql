# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-26 |
| Branch | `issue-333-m181-tls` (M181 TLS, not yet merged) |
| Next step | Finish M181 ([#333](https://github.com/tanbamboo/rusql/issues/333)): merge the TLS PR when CI is green, then label M182 ([#334](https://github.com/tanbamboo/rusql/issues/334)) `agent-ready`. Leave `needs-human` on M182. Do not mass-label later phases. Do not jump to #342. Ultimate MySQL 8.0 goal is **not** complete (Phases X–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence on `main`.

## Status vs goal (2026-10-26)

| Layer | Status |
|-------|--------|
| CI on `main` | Green through M176 (PR #429, `cd67acc`) and HANDOFF PR #434 (`6cc60dc`) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q–W | **Complete** on `main` |
| Phase X M181 | In progress on `issue-333-m181-tls`. ADR accepted: [adr-m181-tls.md](docs/en/architecture/adr-m181-tls.md). `--ssl-cert` / `--ssl-key` present the certificate. Errno **1043** / **3159**. Not `REQUIRE SSL` |
| Phase X M182–M188 | Open. Stay `needs-human` until their own ADRs |
| Phase Y–Z | Open. [#342](https://github.com/tanbamboo/rusql/issues/342) was already `agent-ready` and is not next |

| Estimated surface | ~45–70% client-visible; remaining work is Phase X through Z (including M209/M210) |

## Recent Progress

- **M181 in progress** — rustls listener. Default plaintext. `CLIENT_SSL` only when PEM cert+key load. `mysql --ssl-mode=REQUIRED` against a cert server completes TLS and the server presents that cert. Without certs, `REQUIRED` fails in the client (2026) because the bit is clear; a raw SSL request is errno **1043**. `--require-secure-transport` rejects plaintext with errno **3159**. Not M184 `REQUIRE SSL` / X509. Keys are not committed (#333)
- **M176 merged** — `@@gtid_executed` / `Executed_Gtid_Set` from applied GTIDs in `{data_dir}/gtid.json`. Empty string on a fresh server. `COMMIT` records produced GTIDs. `START SLAVE` records applied GTIDs. Restart reloads the set. `@@session.gtid_executed` errno **1238**. SET errno **1238**. Not `gtid_mode` UI or Group Replication (#328 / PR #429, `cd67acc`)
- **M180 merged** — `WRITE_ROWS` / `UPDATE_ROWS` / `DELETE_ROWS` v2 with extra-data length. `binlog_row_image` MINIMAL/NOBLOB/FULL. INSERT still logs the full row. Invalid image errno **1231**. Corrupt v2 header errno **1610** (#332 / PR #433)
- **M179 merged** — `CHANGE MASTER TO` / `START SLAVE` / `STOP SLAVE` (and `START REPLICA` synonyms). Replica connects, dumps, and applies row events. `SHOW SLAVE STATUS` IO/SQL flags. Errnos 1200, 1198, 1231, 2003 (#331 / PR #432)
- **M178 merged** — Semi-sync dump request or ACK is errno 1524 (`rpl_semi_sync_master` not loaded). No ACK wait. Durability stays async WAL (#330 / PR #431)
- **M177 merged** — Manual promote: `SET GLOBAL super_read_only` / `read_only`, persisted in `replica-role.json`. Errno 1290 / 1229 / 1231. No automatic failover and no cross-directory fencing (#329 / PR #430)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p rusql-server tls_listen
cargo test -p rusql-protocol ssl_request
cargo test
node scripts/harness-validate.mjs
node scripts/check-changelog.mjs
```

## Notes

- M181 stack is rustls 0.23 (`ring`) + tokio-rustls, TLS 1.2 and 1.3. `--ssl-cert` and `--ssl-key` are both required. `mysql-diff` keeps `--ssl-mode=DISABLED`. Official `mysql --ssl-mode=REQUIRED` is documented in the user guide; `tls_listen` runs it when `mysql` is on PATH and (on CI) `RUSQL_ORACLE_MYSQL=1`.
- M176 format is MySQL `uuid:intervals` (comma-separated groups, `:` between intervals). Bare `@@gtid_executed` and `@@global.gtid_executed` read the persisted set. `@@session.` / `@@local.` are errno **1238**. `SHOW VARIABLES` lists the name. `Retrieved_Gtid_Set` stays empty. Not `gtid_mode` switching, Group Replication, or `mysql.gtid_executed`.
- Do not mass-label Phase X–Z. After M181 merges, the next sequenced issue is M182 ([#334](https://github.com/tanbamboo/rusql/issues/334)): add `agent-ready`, leave `needs-human`. [#342](https://github.com/tanbamboo/rusql/issues/342) (M190) was already `agent-ready` and is not next.
- Overall MySQL 8.0 goal is **not** complete.
