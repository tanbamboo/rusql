# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-26 |
| Branch | `main` after M181 merge — [PR #435](https://github.com/tanbamboo/rusql/pull/435) |
| Next step | Phase X M181 is on `main`. Next sequenced issue is M182 ([#334](https://github.com/tanbamboo/rusql/issues/334)), labeled `agent-ready` and still `needs-human` (roles / authz ADR). Do not mass-label later phases. Do not jump to #342. Ultimate MySQL 8.0 goal is **not** complete (Phases X–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence on `main`.

## Status vs goal (2026-10-26)

| Layer | Status |
|-------|--------|
| CI on `main` | Green through M181 (PR #435) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q–W | **Complete** on `main` |
| Phase X M181 | **Complete** on `main` via PR #435. `--ssl-cert` / `--ssl-key` present the certificate. Plaintext default remains. Errno **1043** / **3159**. Not `REQUIRE SSL` |
| Phase X M182–M188 | Open. M182 ([#334](https://github.com/tanbamboo/rusql/issues/334)) is next: `agent-ready` plus `needs-human` |
| Phase Y–Z | Open. [#342](https://github.com/tanbamboo/rusql/issues/342) was already `agent-ready` and is not next |

| Estimated surface | ~45–70% client-visible; remaining work is Phase X through Z (including M209/M210) |

## Recent Progress

- **M181 merged** — TLS listener. `--ssl-cert` and `--ssl-key` (PEM, both required) load rustls and advertise `CLIENT_SSL`. The server presents that certificate. `mysql --ssl-mode=REQUIRED` connects. Without the flags the greeting omits `CLIENT_SSL`, so REQUIRED fails in the client (2026) and a raw SSL request is errno **1043**. Plaintext login still works, including on a TLS listener. `--require-secure-transport` rejects plaintext with errno **3159** before authentication. Not per-account `REQUIRE SSL` / `REQUIRE X509` (M184). Private keys are not logged or committed. ADR: [adr-m181-tls.md](docs/en/architecture/adr-m181-tls.md) (#333 / PR #435)
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

- M181 stack is rustls 0.23 (`ring`) + tokio-rustls, TLS 1.2 and 1.3. `mysql-diff` keeps `--ssl-mode=DISABLED`. Official `mysql --ssl-mode=REQUIRED` is in the user guide; `tls_listen` runs it when `mysql` is on PATH and (on CI) `RUSQL_ORACLE_MYSQL=1`.
- M176 format is MySQL `uuid:intervals` (comma-separated groups, `:` between intervals). Bare `@@gtid_executed` and `@@global.gtid_executed` read the persisted set. `@@session.` / `@@local.` are errno **1238**. `SHOW VARIABLES` lists the name. `Retrieved_Gtid_Set` stays empty. Not `gtid_mode` switching, Group Replication, or `mysql.gtid_executed`.
- Do not mass-label Phase X–Z. Next sequenced issue is M182 ([#334](https://github.com/tanbamboo/rusql/issues/334)): `agent-ready` and still `needs-human`. [#342](https://github.com/tanbamboo/rusql/issues/342) (M190) was already `agent-ready` and is not next.
- Overall MySQL 8.0 goal is **not** complete.
