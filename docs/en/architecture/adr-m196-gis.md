# ADR: M196 GIS / spatial subset

**Status**: Accepted (M196 / [#349](https://github.com/tanbamboo/rusql/issues/349))  
**Date**: 2026-10-28

## Context

MySQL 8.0 community GIS is a large engine (types, `ST_*`, an EPSG catalog, axis order, and a projection stack). The issue requires an ADR before any separate crate, and forbids a full SRID 3857 proj stack in this slice. The default is to implement a portable subset, not to mark community GIS out of scope. This decision does not change `CONSTITUTION.md`.

## Decision

1. **No new crate.** WKT, WKB, and SRID live in `rusql-core`. `ST_*` evaluation lives in `rusql-executor`. Column `TYPE SRID n` is rewritten in `rusql-sql` so the existing parser can accept it. There is no geography type and no spatial index.

2. **Column types.** `GEOMETRY`, `POINT`, `LINESTRING`, `POLYGON`, `MULTIPOINT`, `MULTILINESTRING`, `MULTIPOLYGON`, and `GEOMETRYCOLLECTION` (`GEOMCOLLECTION` is the stored name, matching MySQL `SHOW CREATE`). `CREATE TABLE t (p POINT)` stores rows. `POINT SRID 4326` requires that SRID on insert (errno **3643** when it differs). An unknown column SRID is errno **3548** at DDL.

3. **Values.** `ST_GeomFromText` / `ST_GeometryFromText` parse 2D WKT. The cell is lowercase hex of a 4-byte little-endian SRID plus ISO WKB. `SELECT` returns that payload. `ST_AsText` / `ST_AsWKT` print MySQL's WKT (`POINT(1 2)`, `LINESTRING(0 0,1 1)`, parenthesized multi-points, no space after commas). A plain WKT string is not a geometry (errno **3037**). Inserting a non-geometry or the wrong subtype into a typed column is errno **1416**.

4. **SRID catalog (no proj).** Accepted SRIDs are **0**, **4326**, and **3857**. Anything else is errno **3548**. A negative or non-numeric SRID is errno **1690**. SRID 4326 is geographic: the first ordinate is latitude `[-90, 90]` (errno **3617**) and the second is longitude `(-180, 180]` (errno **3616**). `ST_SRID(geom, srid)` swaps every vertex when exactly one of the old and new SRIDs is 4326, which is what MySQL's axis order does. SRID 3857 is stored as that code. Coordinates are not transformed into Web Mercator.

5. **Other `ST_*` in this slice.** Type-specific `*FromText` constructors, `ST_SRID`, `ST_GeometryType`, `ST_Dimension`, `ST_IsEmpty`, `ST_X`, `ST_Y`, `ST_Latitude`, and `ST_Longitude`. A constructor given the wrong geometry type is errno **3516**. `ST_Latitude` / `ST_Longitude` on a non-geographic SRID are errno **3726**. The wrong argument count is errno **1582**. NULL in, NULL out.

## Consequences

- `CREATE TABLE` plus `INSERT` … `ST_GeomFromText` plus `SELECT ST_AsText` round-trips a point. mysql-diff suite `gis_spatial` compares that text with MySQL 8.0.
- `HEX(ST_GeomFromText(..., 4326))` does not match MySQL. MySQL stores geographic ordinates swapped inside WKB and prints them back in axis order. rusql stores the printed order. `ST_AsText`, `ST_X`, `ST_Y`, `ST_Latitude`, and `ST_Longitude` match the MySQL 8.0 results checked for this slice.
- Predicates and measures (`ST_Contains`, `ST_Intersects`, `ST_Distance`, `ST_Buffer`, `ST_Area`, `ST_Length`) stay unsupported. The `axis-order=` option argument is not accepted.

## Out of scope

- A proj / EPSG transform stack, including turning SRID 3857 into meters
- The full `ST_SPATIAL_REFERENCE_SYSTEMS` catalog
- Spatial indexes, the `GEOGRAPHY` type, and 3D/4D ordinates
- `POINT EMPTY` (MySQL 8.0 also rejects it with errno **3037**)
- Claiming MySQL 8.0 compatibility is complete

## How to verify

```bash
cargo test -p rusql-executor st_astext
cargo test -p rusql-server st_astext
```

```sql
CREATE TABLE gis_t (id INT PRIMARY KEY, p POINT);
INSERT INTO gis_t VALUES (1, ST_GeomFromText('POINT(1 2)'));
SELECT ST_AsText(p) FROM gis_t;
```
