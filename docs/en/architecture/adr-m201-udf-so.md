# ADR: M201 UDF `.so` — explicit security skip

**Status**: Accepted (M201 / [#354](https://github.com/tanbamboo/rusql/issues/354))  
**Date**: 2026-11-09

## Context

MySQL 8.0 Community supports native user-defined functions installed with `CREATE FUNCTION … SONAME 'lib.so'` (and `CREATE AGGREGATE FUNCTION … SONAME`). That path `dlopen`s a shared library chosen by the SQL client. Loading arbitrary `.so` / `.dll` code into the server process is a trust boundary.

rusql already implements SQL stored functions (`CREATE FUNCTION … RETURNS … BEGIN … END`, M63). Those are not native UDFs. A silent ignore of `SONAME`, or a fake OK that pretends the library loaded, would hide a security gap. `needs-human` was only to confirm **skip** vs a restricted loader. This ADR is that confirmation. It does not change `CONSTITUTION.md`. It does not implement a plugin / component loader (M202).

## Decision

1. **Native UDF shared libraries are out of scope (security).** rusql does **not** `dlopen` user `.so` / `.dll` UDFs in default builds. The same wording is in [rusql-vs-mysql.md](../reports/rusql-vs-mysql.md), the zh-CN mirror, and [SKIPS.md](../../../tests/mysql-test/SKIPS.md).

2. **`CREATE FUNCTION … SONAME` and `CREATE AGGREGATE FUNCTION … SONAME` error.** They are a documented parser reject (errno **1064**) with an i18n message that states native UDF shared libraries are out of scope and are not loaded. Not a silent ignore. Not an OK packet.

3. **SQL stored functions stay in scope.** M63 `CREATE FUNCTION … BEGIN … END` / `DROP FUNCTION` / scalar calls are unchanged. This skip does not remove them.

4. **Not a plugin loader.** `INSTALL PLUGIN` / `UNINSTALL PLUGIN` / the component loader remain M202. This ADR does not claim M202 is done.

5. **Not the ultimate goal.** Documenting this skip finishes M201 only. Phase Z issues M202–M210, including M209 and M210, remain. HANDOFF must not say MySQL 8.0 compatibility is complete.

## Consequences

- mysql-test suites that install native UDFs stay skipped, with this security reason.
- Agents clear `needs-human` for #354 once this ADR is on `main`.
- Later milestones must not mark SQL stored functions as out of scope by citing M201.
- M209 may count native UDF `.so` as **Out of scope** (security). Other missing community behavior stays Missing until it lands or its own ADR skips it.

## Out of scope for this ADR

- A restricted / signed UDF loader
- A `.so` / component plugin loader (M202)
- Changing stored-function semantics (M63)
- Claiming MySQL 8.0 compatibility is complete

## How to verify

```bash
cargo test -p rusql-sql udf_soname_rejected
```

```sql
CREATE FUNCTION myfunc RETURNS INTEGER SONAME 'udf_example.so';
-- Expect errno 1064 (parser reject); library is not loaded.
```
