# Architecture Overview

## System Description

**rusql** is a Rust implementation of a MySQL 8.0-compatible relational database. Development is driven by GitHub Issues and Harness Engineering (spec-first, sensor-gated, issue-loop).

## Workspace Structure

```
crates/
├── rusql-i18n       # i18n (en-US default, zh-CN)
├── rusql-protocol   # MySQL wire protocol
├── rusql-sql        # SQL parser & AST
├── rusql-core       # Catalog, session, types
├── rusql-storage    # StorageEngine trait + engines
├── rusql-executor   # Volcano-style executor
├── rusql-planner    # Query planner
├── rusql-server     # Tokio TCP listener
└── rusql-cli        # Admin / diagnostic CLI
```

## Technology Choices

| Layer | Technology |
|-------|------------|
| Runtime | Tokio |
| Networking | tokio::net (MySQL protocol) |
| SQL parsing | sqlparser (MySQL dialect) |
| Logging | tracing |
| Errors | thiserror (libs), anyhow (bins) |
| i18n | rust-i18n |

## Architecture Principles

1. **Layered crates**: protocol → session → sql → planner → executor → storage
2. **Trait-based storage**: `StorageEngine` allows swapping heap/B+Tree backends
3. **Issue-scoped PRs**: one `area:*` per PR
4. **i18n from day one**: no hardcoded user strings in `crates/`

| Milestone | Goal | Status |
|-----------|------|--------|
| M0 | Harness bootstrap | Done |
| M1 | Wire protocol handshake | Done |
| M2 | SQL parse + CREATE/SELECT/INSERT | Done |
| M3 | Persistence + basic transactions | Done (WAL; transactions not yet) |
| M4 | B+Tree secondary indexes | Done |
| M5 | MySQL compat test subset | Planned |
| M6+ | Replication, views, stored procedures, … | Planned |

## Related

- [Crate boundaries](boundaries.md)
- [Rust profile](../../profiles/rust/guides.md)
