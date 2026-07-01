# M10 — SHOW TABLES

**Merged**: PR #25 · Issue #24

## Problem

Tools and humans discover schema via **`SHOW TABLES`** and **`SHOW DATABASES`**, not only `SELECT`.

## Design choices

- MySQL-style column names: `Tables_in_rusql`, `Database`
- Single logical database `rusql` (multi-DB deferred)
- Implemented in executor against engine catalog

## Trade-offs

No `SHOW TABLES LIKE`, no `information_schema` yet (M12).

## Harness lesson

> Metadata commands are **cheap compat wins** — high user visibility, low storage risk.
