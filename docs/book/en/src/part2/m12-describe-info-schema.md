# M12 — DESCRIBE and information_schema

**Merged**: PR #30 · Issue #29 · [spec](../../../en/specs/m12-describe-info-schema.md)

## Problem

ORMs and GUI tools introspect via **`DESCRIBE`**, **`SHOW COLUMNS`**, and **`information_schema`**.

## Design choices

- Virtual `information_schema.tables` / `.columns` (no on-disk system DB)
- Fixed schema name `rusql`
- Column types from catalog `Display` strings (lowercased in output)
- `WHERE table_name = '…'` filter on columns only

## Trade-offs

Minimal column sets — not full MySQL information_schema. `EXPLAIN` query plans rejected (use DESCRIBE).

## Harness lesson

> Tooling compat milestones should **reuse catalog truth** — do not fork metadata in a second store.

## See also

- [m12-describe-info-schema.md](../../../en/specs/m12-describe-info-schema.md)
