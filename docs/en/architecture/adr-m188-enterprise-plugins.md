# ADR: M188 Enterprise plugins — explicit skip

**Status**: Accepted (M188 / [#340](https://github.com/tanbamboo/rusql/issues/340))  
**Date**: 2026-10-27

## Context

The MySQL 8.0 goal for rusql is the community server surface. Oracle also ships closed-source Enterprise Edition server plugins. Treating a missing plugin as success, or writing docs that never name it, hides a gap. This issue is the skip list: name the plugins, say they are **out of scope**, and do not claim they are loaded.

The roadmap already excluded closed-source enterprise plugins from the definition of done, provided the exclusion is explicit. `needs-human` was only to confirm that list. This ADR is that confirmation. It does not change `CONSTITUTION.md`. It does not add a plugin loader (M202).

rusql `--audit-log` (M185) is a JSON-lines file. It is not the Enterprise Audit plugin.

## Decision

1. **These Oracle Enterprise plugins and components are out of scope.** They are not in MySQL 8.0 Community Server. rusql does not load them, does not list them as active, and does not return OK for installing them. The names below are the closed list for M188. A name that is not here is not skipped by this ADR.

   | Plugin / component | Product |
   |--------------------|---------|
   | `audit_log` | MySQL Enterprise Audit |
   | `mysql_firewall` | MySQL Enterprise Firewall |
   | `data_masking` | MySQL Enterprise Data Masking and De-identification |
   | `component_masking` | Data masking component form (8.0.33+), including `component_masking_functions` |
   | `thread_pool` | MySQL Enterprise Thread Pool plugin |
   | `authentication_pam` | Enterprise PAM authentication |
   | `authentication_ldap_sasl` | Enterprise LDAP (SASL) |
   | `authentication_ldap_simple` | Enterprise LDAP (simple bind) |
   | `authentication_windows` | Enterprise Windows native authentication |
   | `keyring_okv` | Oracle Key Vault keyring |
   | `keyring_aws` | AWS keyring |
   | `keyring_hashicorp` | HashiCorp Vault keyring |
   | `keyring_oci` | OCI Vault keyring |
   | `enterprise_encryption` | MySQL Enterprise Encryption |
   | `version_tokens` | Enterprise version tokens |

   The same identifiers are in [rusql-vs-mysql.md](../reports/rusql-vs-mysql.md), the zh-CN mirror, and [SKIPS.md](../../../tests/mysql-test/SKIPS.md). Required words in those docs: audit, firewall, data masking, thread pool plugin.

2. **Community plugins that Phases W–Y still require stay in scope.** This skip does not cover them:

   - `performance_schema` (Phase Y, M189)
   - Slow query log, general log, and `--log-error` (M191, M192, M195)
   - `validate_password` (community; built in by M183, not a `.so`)
   - `caching_sha2_password` and `mysql_native_password`
   - `rpl_semi_sync_master` (community semisync, M178)
   - `keyring_file` (community keyring; `keyring_encrypted_file` is also community)
   - `rewriter` and `connection_control` (community)
   - `authentication_kerberos` and `authentication_fido` (community in 8.0)
   - `group_replication` (M199) and `clone` (M200), which are Phase Z community work, not Enterprise plugins
   - UDF `.so` (M201) and `INSTALL PLUGIN` / the component loader (M202)

3. **No loader and no success packet.** This change does not implement `INSTALL PLUGIN`, `UNINSTALL PLUGIN`, or `INSTALL COMPONENT`. It does not add rows that say these plugins are loaded. M202 owns the loader. A fake OK is forbidden.

4. **Not the ultimate goal.** Documenting this skip finishes M188 and Phase X's last issue. Phases Y–Z, including M209 and M210, are still required. HANDOFF must not say the MySQL 8.0 goal is complete.

MySQL Enterprise Backup and MySQL Enterprise Monitor are commercial products, not these server plugins. rusql does not ship them. They are not added to the plugin table, so they are not confused with community plugins.

## Consequences

- mysql-test files that install or assert the plugins in the table stay skipped, with this list as the reason.
- M185 `--audit-log` remains available and is still not `audit_log`.
- Later milestones must not mark `performance_schema`, semisync, `validate_password`, `group_replication`, or `clone` as out of scope by citing M188.
- M209 may count only this table as **out of scope**. Other missing community behavior stays Missing until it lands or its own ADR skips it.

## Out of scope for this ADR

- A `.so` / component loader (M202)
- Declaring community UDF `.so` out of scope (M201)
- Implementing firewall rules, masking functions, or a thread-pool scheduler
- Changing `CONSTITUTION.md`

## How to verify

```bash
node scripts/doc-parity.mjs
```

Each plugin identifier in the table above appears in `docs/en/reports/rusql-vs-mysql.md`, `docs/zh-CN/reports/rusql-vs-mysql.md`, and `tests/mysql-test/SKIPS.md`. No crate claims these plugins are loaded.
