# M15 — USE database

**Merged**: (pending) · Issue #36

## Problem

MySQL clients run `USE db` after connect to select a default schema before other commands.

## Design choices

- `Session.database` field (default `rusql`)
- Accept `USE rusql`, `USE DATABASE rusql`, `USE SCHEMA rusql`
- Reject unknown database names (single-DB MVP)

## Trade-offs

No multi-database storage — `USE` is session state only until M16+ needs real catalogs per DB.

## Harness lesson

> Session fields belong in **rusql-core** early; executor sets them, info_schema reads them.

## See also

- [m15-use-database.md](../../../en/specs/m15-use-database.md)
