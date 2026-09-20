## Goal

Persist event `DEFINER` and `ON COMPLETION` so dumps and `SHOW CREATE EVENT` match phpMyAdmin/MySQL shape, and one-time `AT` events with `ON COMPLETION PRESERVE` stay in the catalog (DISABLED) after they run.

## Background

Phase Q after M106. `SHOW EVENTS` `Definer` is the stub `root@%`. `SHOW CREATE EVENT` omits `DEFINER` and `ON COMPLETION`. M104 always drops due `AT` events (`NOT PRESERVE`). Client dumps send `CREATE DEFINER=\`u\`@\`h\` EVENT … ON COMPLETION NOT PRESERVE … DO …`. This slice stores `EventMeta.definer` and `on_completion` (`serde(default)`), reconstructs them, and changes only the `AT` completion path. Last-executed on `SHOW EVENTS`, COMMENT, `DISABLE ON SLAVE`, timer thread, and dropping `EVERY` after `ENDS` stay later.

## Acceptance Criteria

- [ ] `CREATE [DEFINER = user] EVENT … [ON COMPLETION [NOT] PRESERVE] … DO` persists definer (`user@host`; omitted → session user/host) and on_completion (`PRESERVE` / `NOT PRESERVE`; omitted → `NOT PRESERVE`)
- [ ] `ALTER EVENT name [DEFINER = user] [ON COMPLETION [NOT] PRESERVE]` updates the catalog
- [ ] `SHOW CREATE EVENT` reconstructs `DEFINER=\`u\`@\`h\`` and `ON COMPLETION PRESERVE|NOT PRESERVE`. `SHOW EVENTS` `Definer` comes from the catalog. Column count stays 15
- [ ] Due ENABLED `ONE TIME` `AT`: `NOT PRESERVE` still drops after a successful `DO` (M104). `PRESERVE` keeps the row and sets `DISABLED`
- [ ] M106 `STARTS`/`ENDS` gates, M105 interval watermark, and `SHOW CREATE USER` from M99 are unchanged. `programs.json` without `definer`/`on_completion` still loads
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false`; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-core/src/**` (`EventMeta.definer` / `on_completion` with `serde(default)`)
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (`DEFINER` / `ON COMPLETION` parse on events only)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- `SHOW ENGINE INNODB STATUS` / `SHOW ENGINE MUSQL STATUS`
- Background tokio timer / sleep-based tests
- Last-executed column on `SHOW EVENTS`
- COMMENT / `DISABLE ON SLAVE` persistence
- Dropping `EVERY` events after `ENDS`
- `SET GLOBAL event_scheduler`
- Generating live truncation / strict-mode warnings
- New crates
- DEFINER on procedures / functions / triggers / views

## Negative Constraints

- Do not claim full MySQL 8.0 event-scheduler parity; document catalog DEFINER / ON COMPLETION without a timer thread
- Do not change M104 `NOT PRESERVE` drop-after-run, M105 watermark, or M106 `STARTS`/`ENDS` gates
- Do not invent last-executed cells on `SHOW EVENTS`
- Do not implement GTID event type 33 / heartbeat

## Test plan

```bash
cargo test -p rusql-sql create_event
cargo test -p rusql-sql alter_event
cargo test -p rusql-core programs
cargo test -p rusql-executor event_scheduler
cargo test -p rusql-executor show_create_event
cargo test -p rusql-server event_scheduler
```
