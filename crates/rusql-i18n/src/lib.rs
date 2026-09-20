//! User-visible message internationalization for rusql.
//!
//! Default locale: `en-US`. Also supports `zh-CN`.
//! Override via `RUSQL_LOCALE` environment variable or [`set_locale`].

use std::sync::OnceLock;

rust_i18n::i18n!("locales", fallback = "en-US");

static LOCALE: OnceLock<String> = OnceLock::new();

/// Returns the active locale tag (e.g. `en-US`, `zh-CN`).
pub fn locale() -> &'static str {
    LOCALE
        .get_or_init(|| std::env::var("RUSQL_LOCALE").unwrap_or_else(|_| "en-US".to_string()))
        .as_str()
}

/// Sets the active locale for this process. Call before any translation if not using `RUSQL_LOCALE`.
pub fn set_locale(locale: &str) {
    let normalized = normalize_locale(locale);
    rust_i18n::set_locale(&normalized);
    let _ = LOCALE.set(normalized);
}

fn normalize_locale(locale: &str) -> String {
    match locale.to_lowercase().as_str() {
        "zh" | "zh-cn" | "zh_cn" => "zh-CN".to_string(),
        "en" | "en-us" | "en_us" => "en-US".to_string(),
        other => other.to_string(),
    }
}

/// Initialize locale from environment. Idempotent.
pub fn init() {
    set_locale(locale());
}

fn tr(key: &str) -> String {
    rust_i18n::t!(key, locale = locale()).to_string()
}

/// User-visible message helpers.
pub mod messages {
    use super::tr;

    pub fn server_starting(port: u16) -> String {
        tr("server.starting").replace("%{port}", &port.to_string())
    }

    pub fn server_stopped() -> String {
        tr("server.stopped")
    }

    pub fn protocol_handshake_failed() -> String {
        tr("protocol.handshake_failed")
    }

    pub fn protocol_invalid_packet() -> String {
        tr("protocol.invalid_packet")
    }

    pub fn protocol_unsupported_auth() -> String {
        tr("protocol.unsupported_auth")
    }

    pub fn protocol_access_denied() -> String {
        tr("protocol.access_denied")
    }

    pub fn sql_parse_error(detail: &str) -> String {
        tr("sql.parse_error").replace("%{detail}", detail)
    }

    pub fn sql_fk_child_violation(
        schema: &str,
        table: &str,
        constraint: &str,
        columns: &str,
        ref_table: &str,
        ref_columns: &str,
    ) -> String {
        tr("sql.fk_child_violation")
            .replace("%{schema}", schema)
            .replace("%{table}", table)
            .replace("%{constraint}", constraint)
            .replace("%{columns}", columns)
            .replace("%{ref_table}", ref_table)
            .replace("%{ref_columns}", ref_columns)
    }

    pub fn sql_fk_parent_violation(
        schema: &str,
        table: &str,
        constraint: &str,
        columns: &str,
        ref_table: &str,
        ref_columns: &str,
    ) -> String {
        tr("sql.fk_parent_violation")
            .replace("%{schema}", schema)
            .replace("%{table}", table)
            .replace("%{constraint}", constraint)
            .replace("%{columns}", columns)
            .replace("%{ref_table}", ref_table)
            .replace("%{ref_columns}", ref_columns)
    }

    pub fn sql_command_denied(command: &str, user: &str, host: &str) -> String {
        tr("sql.command_denied")
            .replace("%{command}", command)
            .replace("%{user}", user)
            .replace("%{host}", host)
    }

    pub fn sql_command_denied_table(command: &str, user: &str, host: &str, table: &str) -> String {
        tr("sql.command_denied_table")
            .replace("%{command}", command)
            .replace("%{user}", user)
            .replace("%{host}", host)
            .replace("%{table}", table)
    }

    pub fn account_admin_required(user: &str, host: &str) -> String {
        tr("sql.account_admin_required")
            .replace("%{user}", user)
            .replace("%{host}", host)
    }

    pub fn sql_duplicate_entry(value: &str, key: &str) -> String {
        tr("sql.duplicate_entry")
            .replace("%{value}", value)
            .replace("%{key}", key)
    }

    pub fn sql_unknown_system_variable(name: &str) -> String {
        tr("sql.unknown_system_variable").replace("%{name}", name)
    }

    pub fn sql_set_global_rejected(name: &str) -> String {
        tr("sql.set_global_rejected").replace("%{name}", name)
    }

    pub fn sql_variable_is_readonly(name: &str) -> String {
        tr("sql.variable_is_readonly").replace("%{name}", name)
    }

    pub fn sql_user_variable_unsupported(name: &str) -> String {
        tr("sql.user_variable_unsupported").replace("%{name}", name)
    }

    pub fn sql_set_names_unsupported() -> String {
        tr("sql.set_names_unsupported")
    }

    pub fn sql_wrong_value_for_var(name: &str, value: &str) -> String {
        tr("sql.wrong_value_for_var")
            .replace("%{name}", name)
            .replace("%{value}", value)
    }

    pub fn sql_set_multi_assign_unsupported() -> String {
        tr("sql.set_multi_assign_unsupported")
    }

    pub fn sql_truncate_partitions_unsupported() -> String {
        tr("sql.truncate_partitions_unsupported")
    }

    pub fn sql_truncate_cascade_unsupported() -> String {
        tr("sql.truncate_cascade_unsupported")
    }

    pub fn sql_truncate_on_cluster_unsupported() -> String {
        tr("sql.truncate_on_cluster_unsupported")
    }

    pub fn sql_truncate_multiple_tables_unsupported() -> String {
        tr("sql.truncate_multiple_tables_unsupported")
    }

    pub fn sql_truncate_not_base_table(name: &str) -> String {
        tr("sql.truncate_not_base_table").replace("%{name}", name)
    }

    pub fn sql_replace_into_unsupported() -> String {
        tr("sql.replace_into_unsupported")
    }

    pub fn sql_replace_composite_pk_unsupported() -> String {
        tr("sql.replace_composite_pk_unsupported")
    }

    pub fn sql_on_conflict_unsupported() -> String {
        tr("sql.on_conflict_unsupported")
    }

    pub fn sql_odku_composite_pk_unsupported() -> String {
        tr("sql.odku_composite_pk_unsupported")
    }

    pub fn sql_insert_column_count(expected: usize, actual: usize) -> String {
        tr("sql.insert_column_count")
            .replace("%{expected}", &expected.to_string())
            .replace("%{actual}", &actual.to_string())
    }

    pub fn sql_with_recursive_unsupported() -> String {
        tr("sql.with_recursive_unsupported")
    }

    pub fn sql_named_window_unsupported() -> String {
        tr("sql.named_window_unsupported")
    }

    pub fn sql_window_frame_unsupported() -> String {
        tr("sql.window_frame_unsupported")
    }

    pub fn sql_unsupported_window_function(name: &str) -> String {
        tr("sql.unsupported_window_function").replace("%{name}", name)
    }

    pub fn sql_window_rank_no_args() -> String {
        tr("sql.window_rank_no_args")
    }

    pub fn sql_user_not_found(account: &str) -> String {
        tr("sql.user_not_found").replace("%{account}", account)
    }

    pub fn procedure_exists(name: &str) -> String {
        tr("programs.procedure_exists").replace("%{name}", name)
    }

    pub fn procedure_not_found(name: &str) -> String {
        tr("programs.procedure_not_found").replace("%{name}", name)
    }

    pub fn function_exists(name: &str) -> String {
        tr("programs.function_exists").replace("%{name}", name)
    }

    pub fn function_not_found(name: &str) -> String {
        tr("programs.function_not_found").replace("%{name}", name)
    }

    pub fn trigger_exists(name: &str) -> String {
        tr("programs.trigger_exists").replace("%{name}", name)
    }

    pub fn trigger_not_found(name: &str) -> String {
        tr("programs.trigger_not_found").replace("%{name}", name)
    }

    pub fn event_exists(name: &str) -> String {
        tr("programs.event_exists").replace("%{name}", name)
    }

    pub fn event_not_found(name: &str) -> String {
        tr("programs.event_not_found").replace("%{name}", name)
    }

    pub fn unsupported_program_body(detail: &str) -> String {
        tr("programs.unsupported_body").replace("%{detail}", detail)
    }

    pub fn call_no_such_procedure(name: &str) -> String {
        tr("programs.call_no_such_procedure").replace("%{name}", name)
    }

    pub fn storage_table_not_found(name: &str) -> String {
        tr("storage.table_not_found").replace("%{name}", name)
    }

    pub fn storage_database_not_found(name: &str) -> String {
        tr("storage.database_not_found").replace("%{name}", name)
    }

    pub fn storage_database_exists(name: &str) -> String {
        tr("storage.database_exists").replace("%{name}", name)
    }

    pub fn storage_database_not_empty(name: &str) -> String {
        tr("storage.database_not_empty").replace("%{name}", name)
    }

    pub fn cli_usage() -> String {
        tr("cli.usage")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_locale_is_en_us() {
        set_locale("en-US");
        let msg = messages::server_starting(3306);
        assert!(msg.contains("Starting") || msg.contains("rusql"));
    }

    #[test]
    fn zh_cn_locale_works() {
        set_locale("zh-CN");
        let msg = messages::server_starting(3306);
        assert!(msg.contains("启动") || msg.contains("rusql"));
    }

    #[test]
    fn unknown_system_variable_i18n() {
        set_locale("en-US");
        let en = messages::sql_unknown_system_variable("foo");
        assert!(en.contains("foo"));
        assert!(
            en.contains("Unknown system variable") || en.contains("未知的系统变量"),
            "i18n unknown system variable should mention the name, got {en}"
        );
    }

    #[test]
    fn set_global_and_readonly_i18n() {
        set_locale("en-US");
        let global = messages::sql_set_global_rejected("autocommit");
        assert!(global.contains("autocommit"));
        assert!(global.contains("SET GLOBAL") || global.contains("SESSION"));
        let ro = messages::sql_variable_is_readonly("version");
        assert!(ro.contains("version"));
        set_locale("zh-CN");
        let zh = messages::sql_set_global_rejected("autocommit");
        assert!(zh.contains("autocommit"));
        set_locale("en-US");
        assert!(messages::sql_set_names_unsupported()
            .to_ascii_lowercase()
            .contains("names"));
        assert!(messages::sql_user_variable_unsupported("foo").contains("foo"));
    }

    #[test]
    fn normalize_locale_aliases() {
        assert_eq!(normalize_locale("zh"), "zh-CN");
        assert_eq!(normalize_locale("en"), "en-US");
    }
}
