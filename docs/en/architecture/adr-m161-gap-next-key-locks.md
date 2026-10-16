# ADR: M161 Gap / next-key locks (InnoDB RR MVP)

**Status**: Accepted (M161 / [#313](https://github.com/tanbamboo/rusql/issues/313))  
**Date**: 2026-10-17

## Context

MySQL InnoDB under **REPEATABLE READ** uses **next-key locks** (record lock + gap lock before the record) on locking reads to prevent **phantom inserts** into scanned ranges. rusql already has M157 row exclusive locks for `SELECT … FOR UPDATE` and M160 deadlock detection on row waits.

M161 adds a minimal gap-lock layer so a **range** `FOR UPDATE` (or SERIALIZABLE implicit lock) blocks concurrent **INSERT** of primary keys inside that range.

## Terminology (InnoDB vs rusql MVP)

| Concept | InnoDB | rusql M161 |
|---------|--------|------------|
| Record lock | Exclusive lock on an existing index record | Existing `RowKey` exclusive lock (M157) |
| Gap lock | Lock on an open interval between index records | `GapLockKey`: bounded interval on the single PK column |
| Next-key lock | Gap before record + record | Row locks on scanned rows **plus** one interval gap lock for the scan predicate |
| Insert intention | Special lock mode | INSERT waits on conflicting gap/row locks, then takes row lock on new PK |

## Decision

1. **Scope**: Tables with a **single-column PRIMARY KEY** only (same as M157). Integer PK values use numeric ordering; other types use string ordering.

2. **When gap locks are taken**: On `SELECT … FOR UPDATE` (including SERIALIZABLE implicit `FOR UPDATE`) when isolation is **REPEATABLE READ** or **SERIALIZABLE** and the `WHERE` clause is a **PK range**:
   - `BETWEEN low AND high` on the PK column, or
   - `pk >= low AND pk <= high` (or `>`, `<` variants) on the PK column.

   Equality-only predicates keep **record locks only** (no extra gap interval).

3. **Gap lock shape**: One inclusive/exclusive interval `[low, high]` per range scan, stored in the shared `RowLockRegistry` alongside row locks. Scanned rows still get record locks.

4. **INSERT / REPLACE (new row)**: Before writing, the inserter **waits** (same `innodb_lock_wait_timeout`, errno **1205**) if another session holds a gap lock covering the new PK, or a row lock on that PK. Same-session reentrancy is allowed.

5. **Deadlocks**: M160 wait-for graph is extended to **gap lock targets**; errno **1213** on the younger session in a cycle.

6. **READ COMMITTED**: No gap locks on locking reads (record locks only), matching InnoDB’s RC behavior for phantoms on inserts.

## Consequences

- Full-table scans without a PK range predicate do **not** take a table-wide gap lock (M159 SERIALIZABLE plain `SELECT` remains row-lock MVP except when a range predicate is present).
- No `FOR SHARE` gap locks, predicate locks, or `SKIP LOCKED` / `NOWAIT` gap behavior in this slice.
- Multi-column PK, non-literal range bounds, and `OR`/`IN` range predicates remain future work.

## Out of scope

- Complete InnoDB lock compatibility matrix
- SERIALIZABLE predicate locking beyond explicit/implied locking reads
- `SHOW ENGINE INNODB STATUS` lock reporting
