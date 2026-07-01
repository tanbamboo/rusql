# The MySQL compatibility landscape

Before diving into milestones, we need a shared map of what **“MySQL 8.0 compatible”** means in practice — and what rusql deliberately does *not* try to be on day one.

## Three surfaces clients care about

| Surface | What breaks if wrong | rusql strategy (M0–M16) |
|---------|----------------------|-------------------------|
| **Wire protocol** | Drivers cannot connect | Protocol v10, OK/ERR, text resultset, `COM_STMT_*` |
| **SQL subset** | ORMs emit unsupported syntax | Incremental executor; `sqlparser` MySQL dialect |
| **Metadata** | Tools introspect empty schema | `information_schema`, `DESCRIBE`, `SHOW` |

Full MySQL includes replication, privilege graphs, optimizers, and hundreds of edge cases. rusql uses **vertical slices** so each merge adds a testable surface without pretending the rest exists.

## Layered architecture (conceptual)

```
Client (mysql CLI, JDBC, SQLAlchemy)
        │ TCP
        ▼
┌───────────────────┐
│ Wire protocol     │  handshake, auth plugins, packet framing
├───────────────────┤
│ Session / catalog │  per-connection state, table metadata
├───────────────────┤
│ Parser & planner  │  AST → plan (MVP: pass-through)
├───────────────────┤
│ Executor          │  volcano-style operators
├───────────────────┤
│ Storage engine    │  heap + WAL + secondary B+Tree (MVP)
└───────────────────┘
```

This mirrors classical RDBMS texts (see [Bibliography](../appendix/bibliography.md)): **storage** handles durability and indexing; **execution** maps relational algebra to iterators; **front-end** speaks the client language.

## Why not fork MySQL?

We want a **small, auditable Rust codebase** suitable for Harness Engineering (agents + sensors), not a C++ lineage port. Trade-offs:

- **Pros**: Clear crate boundaries, memory safety, fast CI, teachable size
- **Cons**: Multi-year feature gap vs Oracle MySQL; behavioral parity requires differential testing (roadmap M29–M30)

## Academic context

- **System R / relational model** — Codd (1970); foundation for SQL tables and catalogs.
- **ARIES recovery** — Mohan et al. (1992); informs our WAL direction (M3 skeleton, M31 durable COMMIT).
- **B+Trees** — Comer (1979); secondary indexes (M4).
- **Transaction isolation** — Berenson et al. (1995); guides M9 overlay vs future M32 MVCC.
- **MySQL internals** — Oracle reference manual + *Understanding MySQL Internals* (Harrison) for protocol/auth context.

## How this book uses theory

We cite papers and manuals when they explain **why** a milestone exists — not to reproduce proofs. Implementation detail stays in repository specs; this book explains **design pressure** and **harness feedback**.

## Roadmap beyond M16

See [mysql-compat-roadmap.md](../../../en/specs/mysql-compat-roadmap.md) for phased issues M17–M35 (query surface, DDL, binary protocol, differential compat, MVCC, replication).
