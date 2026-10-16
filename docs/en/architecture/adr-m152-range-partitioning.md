# ADR: M152 RANGE table partitioning (on-disk layout)

**Status**: Accepted (M152 / [#304](https://github.com/tanbamboo/rusql/issues/304))  
**Date**: 2026-10-16

## Context

MySQL 8.0 supports `CREATE TABLE … PARTITION BY RANGE`. rusql needs a minimal RANGE implementation with correct INSERT routing and SELECT partition pruning (equality on the partition key), without HASH/LIST/KEY or subpartitions.

sqlparser’s MySQL dialect does not parse MySQL’s `(PARTITION p … VALUES LESS THAN …)` clause; rusql-sql strips and validates that clause before AST parse (same pattern as functional indexes).

## Decision

1. **Catalog**: `TableMeta.range_partition` holds `RangePartitionMeta` — partition key column name and ordered `VALUES LESS THAN` bounds (including `MAXVALUE` on the last partition).

2. **Physical layout (heap / WAL)**: One logical table key in `meta` (schema-qualified storage key). Row data lives in **per-partition heap buckets** at `{table_key}\x1f{partition_name}`. The logical table key does not store rows when partitioning is enabled.

3. **INSERT**: Evaluate the partition key from the row; insert into exactly one partition bucket. Values with no matching partition return MySQL-class error (no silent default partition).

4. **SELECT / scan**: Full table scan concatenates all partition buckets in catalog order. When the executor (or EXPLAIN) can prove equality on the partition key, only the matching bucket(s) are read.

5. **Indexes on partitioned tables (MVP)**: No cross-partition secondary index maintenance; equality on the partition key uses prune math. PRIMARY KEY equal to the partition key is supported via the same path.

6. **WAL**: `CreateTable` records include `range_partition` so replay creates the same catalog and empty partition buckets.

## Consequences

- HASH/LIST/KEY and `ALTER TABLE … ADD/DROP PARTITION` remain future work.
- DELETE/UPDATE on partitioned tables scan all partition buckets (correctness over prune for non-key predicates).
- EXPLAIN exposes prune via the `extra` column (`partitions: p0`) and reduced row estimates when pruning applies.

## Out of scope

- Subpartitions, partition exchange, `TRUNCATE PARTITION`, non-INT partition keys (MVP targets integer `VALUES LESS THAN` literals).
