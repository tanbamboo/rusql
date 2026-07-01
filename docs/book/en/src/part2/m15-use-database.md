# M15 — USE database

**Merged**: PR #37 · Issue #36

## Problem

MySQL clients assume a **current database** (schema) per session. Metadata queries (`information_schema`, `SHOW TABLES`) and unqualified table names resolve against it. Without `USE`, tools like ORMs and GUI clients cannot mirror production connection strings.

## Design space

| Approach | Pros | Cons |
|----------|------|------|
| `Session.database` string on core session | Tiny state; easy for info_schema | Must thread through executor |
| Implicit single schema forever | No `USE` code | Breaks MySQL clients |
| Full multi-database catalog | Realistic | Large scope for MVP |

## Decision

- Store `session.database` in `rusql-core` (default `rusql`).
- Accept `USE rusql` (MySQL dialect); reject unknown database names with error.
- `information_schema` filters and `SHOW TABLES` column names use session schema.

## Internals

```
USE rusql → session.database = "rusql"
information_schema.tables → TABLE_SCHEMA = session.database
```

**Note:** `USE DATABASE name` is not parsed by our sqlparser MySQL dialect configuration; clients using bare `USE name` work.

## Trade-offs

Single logical database (`rusql`) for now — enough for compat harness and local dev. Multi-tenant catalogs deferred to roadmap metadata phase (M27–M28).

## Further reading

- MySQL 8.0 Reference: [USE statement](https://dev.mysql.com/doc/refman/8.0/en/use.html)
- Gray & Reuter, *Transaction Processing* — schema as namespace

## Harness lesson

> Session state changes need **both** executor unit tests and a wire-level `USE` step in compat JSON — handshake does not set database automatically.

## See also

- [m15-use-database.md](../../../en/specs/m15-use-database.md)
