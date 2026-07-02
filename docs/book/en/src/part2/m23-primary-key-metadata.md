# M23 — PRIMARY KEY metadata

**Issue #46**

## Problem

ORMs and migrations read `DESCRIBE` / `information_schema` to discover keys and nullability. Without `PRIMARY KEY` and `NOT NULL` in catalog metadata, clients cannot validate schemas.

## Decision

- Extend `ColumnDef` with `nullable` and `primary_key`.
- Parse column options (`NOT NULL`, `PRIMARY KEY`) and table-level `PRIMARY KEY (cols)`.
- Reflect in DESCRIBE (`Null`, `Key`), `SHOW CREATE TABLE`, and `information_schema.columns`.

## Harness lesson

> `primary_key_metadata` compat suite locks DESCRIBE + SHOW CREATE output together.
