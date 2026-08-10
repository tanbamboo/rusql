# M35 — utf8mb4 charset metadata

**Issue #58**

## Problem

MySQL 8.0 clients and ORMs read charset/collation from the handshake and `information_schema`. Wrong or missing metadata causes subtle driver bugs even when SQL execution is UTF-8 safe.

## Decision

- Handshake charset byte **45** (`utf8mb4`).
- `information_schema.columns` adds `COLUMN_COLLATION` (`utf8mb4_unicode_ci`).
- Column-definition packets use utf8mb4 charset id on the wire.

## Harness lesson

> **Metadata milestones** are cheap compat wins — fix handshake and virtual tables before full collation engine work.
