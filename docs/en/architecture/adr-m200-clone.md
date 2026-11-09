# ADR: M200 Clone / data-directory snapshot

**Status**: Accepted (M200 / [#353](https://github.com/tanbamboo/rusql/issues/353))  
**Date**: 2026-11-09

## Context

MySQL 8.0 Community ships a `clone` plugin that supports `CLONE INSTANCE` (remote donor) and `CLONE LOCAL DATA DIRECTORY` for provisioning replicas. rusql already has async binlog apply (M179) and crash-safe WAL replay (M198). Loading an Oracle plugin `.so` into `mysql.plugin` is out of scope (and M188 forbids pretending Enterprise plugins are loaded; community `clone` stays in scope without a fake plugin row). A silent skip is forbidden. Default AC preference is a rusql clone/snapshot command, not only a docs substitute. This decision does not change `CONSTITUTION.md`.

## Decision

1. **Implement a file-level data-directory snapshot**, not a loaded clone plugin and not a `mysql.plugin` row.

2. **SQL subset.** `CLONE LOCAL DATA DIRECTORY [=] 'path'` (and the local alias `CLONE INSTANCE DATA DIRECTORY [=] 'path'`) copies the server's current `--data-dir` into an empty destination directory (or creates that directory). Suitable for seeding a replica data directory. Remote `CLONE INSTANCE FROM 'user'@'host':port …` is rejected with a clear parse error (errno **1064**).

3. **Mechanics.** Before copy, fsync the open WAL when present. Recursively copy regular files and directories under the source data directory only. Do not follow symlinks out of the data directory. Do not copy secrets from outside the data directory. Destination must be empty or absent; non-empty is errno **3869**. I/O failures are errno **3865**.

4. **No global lock.** Clone does not acquire a global read lock. Concurrent writers may produce a torn snapshot. Quiesce writers or stop the donor before cloning when a consistent replica seed is required. Document this in the user guide.

5. **Not** Oracle clone plugin wire protocol, not automatic Group Replication distributed recovery, and not M201/M202.

## Consequences

- Agents clear `needs-human` for #353 once this ADR is on `main`.
- A storage fixture proves `clone_instance` copies a WAL and replays committed rows on the destination.
- Compatibility docs state this is a local data-dir snapshot, not a loaded plugin.
- M210 still requires later Phase Z work; accepting this ADR does not complete MySQL 8.0.

## Out of scope

- Loading `clone` into `mysql.plugin`
- Remote donor `CLONE INSTANCE FROM`
- Clone plugin network protocol and progress tables
- Automatic GR distributed recovery via clone
- M201 UDF / M202 `INSTALL PLUGIN` in the same change
- Claiming MySQL 8.0 compatibility is complete

## How to verify

```bash
cargo test -p rusql-storage clone_instance
cargo test -p rusql-executor clone
cargo test -p rusql-server clone_instance
```

```sql
CLONE LOCAL DATA DIRECTORY = '/path/to/empty/or/new/dir';
-- Then start another rusql with --data-dir pointing at that path.
```
