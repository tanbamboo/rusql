# M33 — SQL views

**Issue #56**

## Problem

Clients expect `CREATE VIEW v AS SELECT …` and `SELECT * FROM v` for read-only indirection. Without views, compatibility reports and information_schema probes fail early.

## Decision

- Store view definitions (`name`, SQL text) in the session catalog — not on disk in MVP.
- `SELECT FROM view` re-parses and executes the stored query.
- Expose `information_schema.VIEWS` stub and mark views in `information_schema.tables`.

## Trade-offs

Views are session-scoped today: they do not survive server restart. That matches the catalog MVP and keeps WAL unchanged.

## Harness lesson

> **Catalog-first** features (views, info_schema) can ship before durable metadata if wire tests and compat JSON define “done.”
