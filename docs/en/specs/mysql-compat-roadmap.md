# MySQL 8.0 Compatibility Roadmap

**Goal**: A credible MySQL 8.0 wire + SQL subset for real clients (ORMs, `mysql` CLI, drivers), developed via Harness Engineering.

**Current**: M0–M16 merged on `main` (~5–10% of full MySQL surface).

**Strategy**: Vertical slices per milestone → one issue → one PR. Dependencies enforced in issue bodies; only `agent-ready` issues are picked by the agent loop.

---

## Dependency graph (high level)

```mermaid
flowchart TB
  subgraph done [Done M0-M16]
    M2[M2 COM_QUERY]
    M3[M3 WAL]
    M4[M4 Indexes]
    M5[M5 Compat fixtures]
    M9[M9 Txn overlay]
    M12[M12 info_schema]
    M14[M14 Projection]
    M16[M16 LIMIT]
  end

  subgraph phaseA [Phase A Query]
    M17[M17 ORDER BY]
    M18[M18 Aliases]
    M19[M19 OFFSET]
    M20[M20 WHERE ops]
    M21[M21 NULL]
    M22[M22 JOIN]
  end

  subgraph phaseB [Phase B DDL]
    M23[M23 PK metadata]
    M24[M24 ALTER TABLE]
  end

  subgraph phaseC [Phase C Protocol]
    M25[M25 Binary resultset]
    M26[M26 RSA auth]
  end

  subgraph phaseD [Phase D Metadata]
    M27[M27 info_schema++]
    M28[M28 SHOW INDEX]
  end

  subgraph phaseE [Phase E Compat feedback]
    M29[M29 mysql-diff]
    M30[M30 mysql-test subset]
  end

  subgraph phaseF [Phase F Storage]
    M31[M31 Durable txn]
    M32[M32 MVCC]
  end

  subgraph phaseG [Phase G Advanced]
    M33[M33 Views]
    M34[M34 Binlog]
    M35[M35 Charset]
  end

  M14 --> M17
  M14 --> M18
  M16 --> M19
  M4 --> M20
  M20 --> M21
  M14 --> M22
  M20 --> M22
  M2 --> M23
  M23 --> M24
  M1 --> M25
  M7 --> M26
  M12 --> M27
  M4 --> M28
  M5 --> M29
  M29 --> M30
  M3 --> M31
  M9 --> M31
  M31 --> M32
  M22 --> M33
  M31 --> M34
  M12 --> M35
```

---

## Phase A — SQL query surface

| ID | Title | Depends on | Priority |
|----|-------|------------|----------|
| M17 | ORDER BY single column | M14 | P0 |
| M18 | SELECT column aliases (`AS`) | M14 | P0 |
| M19 | LIMIT OFFSET | M16 | P1 |
| M20 | WHERE comparisons and AND | M4 | P0 |
| M21 | IS NULL / IS NOT NULL | M20 | P1 |
| M22 | INNER JOIN two tables | M14, M20 | P0 |

## Phase B — DDL & constraints

| ID | Title | Depends on | Priority |
|----|-------|------------|----------|
| M23 | PRIMARY KEY / NOT NULL metadata | M2 | P1 |
| M24 | ALTER TABLE ADD COLUMN | M23 | P1 |

## Phase C — Wire protocol

| ID | Title | Depends on | Priority |
|----|-------|------------|----------|
| M25 | Binary resultset (COM_STMT) | M11 | P1 |
| M26 | caching_sha2 RSA full auth | M7 | P2 |

## Phase D — Information schema

| ID | Title | Depends on | Priority |
|----|-------|------------|----------|
| M27 | information_schema.schemata/statistics | M12 | P1 |
| M28 | SHOW INDEX | M4, M12 | P1 |

## Phase E — Differential compat

| ID | Title | Depends on | Priority |
|----|-------|------------|----------|
| M29 | mysql-diff Docker runner | M5 | P0 |
| M30 | mysql-test subset port | M29 | P2 |

## Phase F — Transactions & durability

| ID | Title | Depends on | Priority |
|----|-------|------------|----------|
| M31 | COMMIT flushes WAL | M3, M9 | P0 |
| M32 | MVCC snapshot isolation | M31 | P2 |

## Phase G — Advanced

| ID | Title | Depends on | Priority |
|----|-------|------------|----------|
| M33 | SQL views (read-only) | M22 | P2 |
| M34 | Binlog replication (ADR) | M31, ADR #5 | P3 |
| M35 | utf8mb4 charset metadata | M12 | P2 |

## Book (parallel)

| ID | Title | Notes |
|----|-------|-------|
| #28 | Professional depth pass | Not blocking code milestones |

---

## Agent loop rules

1. Only issues labeled `agent-ready` are picked.
2. Respect **Depends on** — do not start blocked milestones.
3. After roadmap issue creation, **M17** is first `agent-ready` code issue.
4. Book #28 is `agent-ready` for documentation passes between code milestones.

## Est. compatibility depth

| After phase | Approx. MySQL surface |
|-------------|----------------------|
| M16 (now) | ~8% |
| Phase A | ~15% |
| Phase D | ~20% |
| Phase E | measurable gap vs MySQL |
| Phase F | production-shaped durability |
