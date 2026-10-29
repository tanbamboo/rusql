# ADR：M196 GIS / 空间子集

**状态**：已接受（M196 / [#349](https://github.com/tanbamboo/rusql/issues/349)）  
**日期**：2026-10-28

## 背景

MySQL 8.0 社区版 GIS 是一套大引擎（类型、`ST_*`、EPSG 目录、轴序，以及投影栈）。本议题要求先有 ADR 才能新增 crate，并禁止在这一片实现完整的 SRID 3857 proj 栈。默认是实现可移植子集，而不是把社区 GIS 标成范围外。本决定不修改 `CONSTITUTION.md`。

## 决定

1. **不新增 crate。** WKT、WKB 和 SRID 放在 `rusql-core`。`ST_*` 求值放在 `rusql-executor`。列上的 `TYPE SRID n` 由 `rusql-sql` 改写，现有解析器才能接受。没有 geography 类型，也没有空间索引。

2. **列类型。** `GEOMETRY`、`POINT`、`LINESTRING`、`POLYGON`、`MULTIPOINT`、`MULTILINESTRING`、`MULTIPOLYGON`，以及 `GEOMETRYCOLLECTION`（目录里存成 `GEOMCOLLECTION`，与 MySQL `SHOW CREATE` 一致）。`CREATE TABLE t (p POINT)` 会存行。`POINT SRID 4326` 要求插入值带这个 SRID（不一致是 errno **3643**）。列上的未知 SRID 在 DDL 时是 errno **3548**。

3. **值。** `ST_GeomFromText` / `ST_GeometryFromText` 解析二维 WKT。单元格是小写十六进制：4 字节小端 SRID 加上 ISO WKB。`SELECT` 返回这份载荷。`ST_AsText` / `ST_AsWKT` 按 MySQL 的 WKT 打印（`POINT(1 2)`、`LINESTRING(0 0,1 1)`、多点带括号、逗号后没有空格）。普通 WKT 字符串不是几何（errno **3037**）。把非几何或错误子类型插入带类型的列是 errno **1416**。

4. **SRID 目录（没有 proj）。** 接受的 SRID 是 **0**、**4326** 和 **3857**。其他值是 errno **3548**。负数或非数字 SRID 是 errno **1690**。SRID 4326 是地理坐标：第一个数是纬度 `[-90, 90]`（errno **3617**），第二个数是经度 `(-180, 180]`（errno **3616**）。`ST_SRID(geom, srid)` 在新旧 SRID 里恰好有一个是 4326 时交换每个顶点，这是 MySQL 轴序的行为。SRID 3857 只作为编号存储。坐标不会被变换成 Web Mercator 米制。

5. **这一片里的其他 `ST_*`。** 按类型的 `*FromText` 构造函数、`ST_SRID`、`ST_GeometryType`、`ST_Dimension`、`ST_IsEmpty`、`ST_X`、`ST_Y`、`ST_Latitude`、`ST_Longitude`。构造函数拿到错误几何类型是 errno **3516**。`ST_Latitude` / `ST_Longitude` 用在非地理 SRID 上是 errno **3726**。参数个数不对是 errno **1582**。输入 NULL 则输出 NULL。

## 后果

- `CREATE TABLE`、`INSERT` … `ST_GeomFromText`、再 `SELECT ST_AsText` 能把一个点存回去再读出来。mysql-diff 套件 `gis_spatial` 用这段文本和 MySQL 8.0 比较。
- `HEX(ST_GeomFromText(..., 4326))` 与 MySQL 不一致。MySQL 在 WKB 内部交换地理坐标，再按轴序打印。rusql 按打印顺序存储。`ST_AsText`、`ST_X`、`ST_Y`、`ST_Latitude` 和 `ST_Longitude` 与本片核对过的 MySQL 8.0 结果一致。
- 谓词和度量（`ST_Contains`、`ST_Intersects`、`ST_Distance`、`ST_Buffer`、`ST_Area`、`ST_Length`）仍不支持。不接受 `axis-order=` 选项参数。

## 范围外

- proj / EPSG 变换栈，包括把 SRID 3857 变成米
- 完整的 `ST_SPATIAL_REFERENCE_SYSTEMS` 目录
- 空间索引、`GEOGRAPHY` 类型，以及三维 / 四维坐标
- `POINT EMPTY`（MySQL 8.0 同样以 errno **3037** 拒绝）
- 宣称 MySQL 8.0 兼容已经完成

## 如何验证

```bash
cargo test -p rusql-executor st_astext
cargo test -p rusql-server st_astext
```

```sql
CREATE TABLE gis_t (id INT PRIMARY KEY, p POINT);
INSERT INTO gis_t VALUES (1, ST_GeomFromText('POINT(1 2)'));
SELECT ST_AsText(p) FROM gis_t;
```
