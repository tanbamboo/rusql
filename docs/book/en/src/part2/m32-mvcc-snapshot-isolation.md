# M32 — MVCC snapshot isolation

**Issue #55**

## Problem

M9 gave per-connection transaction overlays, but every statement still took an exclusive lock on the shared engine. A long-running `SELECT` blocked writers, and readers saw commits from other connections immediately — not a repeatable snapshot.

## Decision

- Pin committed row snapshots on first read inside a `BEGIN` transaction (lazy per-table copy).
- Replace `Mutex<PersistentEngine>` with `RwLock` so read-only plans take shared locks.
- Add `ReadOnlyEngine` for autocommit `SELECT` paths.

## Trade-offs

| Choice | Benefit | Cost |
|--------|---------|------|
| Table-grain snapshot copy | Small diff, reuses overlay | Not row-level MVCC on disk |
| RwLock in server | Readers don't block writers | Writers still exclusive |

Full row-version chains and garbage collection remain future work.

## Harness lesson

> Ship isolation semantics with **two-connection wire tests** before optimizing storage layout — the acceptance criteria were behavioral, not structural.
