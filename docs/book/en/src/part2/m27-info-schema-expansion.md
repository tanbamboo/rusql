# M27 — information_schema expansion

**Issue #50**

## Problem

ORMs and `SHOW INDEX` need `information_schema.SCHEMATA` and `STATISTICS`, not only `tables` / `columns` from M12.

## Decision

- `SCHEMATA`: single row for `rusql` with utf8mb4 defaults.
- `STATISTICS`: `PRIMARY` rows from catalog PK metadata + secondary indexes from `StorageEngine::index_metas()`.
- Add `index_metas()` to storage trait for consistent visibility under transactions.

## Harness lesson

> `information_schema_schemata_statistics` compat suite locks SCHEMATA row and STATISTICS rows for PK + secondary index.
