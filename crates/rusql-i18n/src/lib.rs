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

    pub fn sql_cte_max_recursion_depth(depth: u32) -> String {
        tr("sql.cte_max_recursion_depth").replace("%{depth}", &depth.to_string())
    }

    pub fn sql_named_window_unsupported() -> String {
        tr("sql.named_window_unsupported")
    }

    pub fn sql_window_frame_unsupported() -> String {
        tr("sql.window_frame_unsupported")
    }

    pub fn sql_window_frame_illegal() -> String {
        tr("sql.window_frame_illegal")
    }

    pub fn sql_window_frame_offset_invalid() -> String {
        tr("sql.window_frame_offset_invalid")
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

    pub fn sql_unknown_character_set(name: &str) -> String {
        tr("sql.unknown_character_set").replace("%{name}", name)
    }

    pub fn sql_unknown_collation(name: &str) -> String {
        tr("sql.unknown_collation").replace("%{name}", name)
    }

    pub fn sql_invalid_json_text(detail: &str) -> String {
        tr("sql.invalid_json_text").replace("%{detail}", detail)
    }

    pub fn sql_json_extract_arg_count() -> String {
        tr("sql.json_extract_arg_count")
    }

    pub fn sql_incorrect_parameter_count(name: &str) -> String {
        tr("sql.incorrect_parameter_count").replace("%{name}", name)
    }

    pub fn sql_user_lock_wrong_name(name: &str) -> String {
        tr("sql.user_lock_wrong_name").replace("%{name}", name)
    }

    pub fn sql_user_lock_invalid_name(name: &str) -> String {
        tr("sql.user_lock_invalid_name").replace("%{name}", name)
    }

    pub fn sql_binlog_log_not_found(name: &str) -> String {
        tr("sql.binlog_log_not_found").replace("%{name}", name)
    }

    pub fn sql_unknown_storage_engine(name: &str) -> String {
        tr("sql.unknown_storage_engine").replace("%{name}", name)
    }

    pub fn sql_show_engine_innodb_status_stub() -> String {
        tr("sql.show_engine_innodb_status_stub")
    }

    pub fn sql_wrong_object(name: &str, kind: &str) -> String {
        tr("sql.wrong_object")
            .replace("%{name}", name)
            .replace("%{kind}", kind)
    }

    pub fn sql_unknown_prepared_statement(name: &str, command: &str) -> String {
        tr("sql.unknown_prepared_statement")
            .replace("%{name}", name)
            .replace("%{command}", command)
    }

    pub fn sql_execute_using_unsupported() -> String {
        tr("sql.execute_using_unsupported")
    }

    pub fn sql_prepare_nested_unsupported() -> String {
        tr("sql.prepare_nested_unsupported")
    }

    pub fn sql_savepoint_does_not_exist(name: &str) -> String {
        tr("sql.savepoint_does_not_exist").replace("%{name}", name)
    }

    pub fn sql_set_op_column_count_mismatch() -> String {
        tr("sql.set_op_column_count_mismatch")
    }

    pub fn sql_unsupported_set_operator(op: &str) -> String {
        tr("sql.unsupported_set_operator").replace("%{op}", op)
    }

    pub fn sql_intersect_all_unsupported() -> String {
        tr("sql.intersect_all_unsupported")
    }

    pub fn procedure_exists(name: &str) -> String {
        tr("programs.procedure_exists").replace("%{name}", name)
    }

    pub fn procedure_not_found(name: &str) -> String {
        tr("programs.procedure_not_found").replace("%{name}", name)
    }

    pub fn procedure_wrong_arg_count(name: &str, expected: usize, got: usize) -> String {
        tr("programs.procedure_wrong_arg_count")
            .replace("%{name}", name)
            .replace("%{expected}", &expected.to_string())
            .replace("%{got}", &got.to_string())
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
        set_locale("en-US");
        let cs = messages::sql_unknown_character_set("latin1");
        assert!(cs.contains("latin1"));
        let col = messages::sql_unknown_collation("latin1_swedish_ci");
        assert!(col.contains("latin1_swedish_ci"));
        set_locale("en-US");
        let json_en = messages::sql_invalid_json_text("expected value");
        assert!(json_en.contains("expected value"));
        assert!(json_en.to_ascii_lowercase().contains("json"));
        set_locale("zh-CN");
        let json_zh = messages::sql_invalid_json_text("expected value");
        assert!(json_zh.contains("expected value"));
        set_locale("en-US");
        assert!(messages::sql_json_extract_arg_count()
            .to_ascii_lowercase()
            .contains("json_extract"));
        set_locale("en-US");
        let arity_en = messages::sql_incorrect_parameter_count("UUID");
        assert!(arity_en.contains("UUID"));
        assert!(arity_en.to_ascii_lowercase().contains("parameter"));
        set_locale("zh-CN");
        let arity_zh = messages::sql_incorrect_parameter_count("UUID");
        assert!(arity_zh.contains("UUID"));
        set_locale("en-US");
        let lock_en = messages::sql_user_lock_wrong_name("too-long-name");
        assert!(lock_en.contains("too-long-name"));
        assert!(lock_en.to_ascii_lowercase().contains("lock"));
        set_locale("zh-CN");
        let lock_zh = messages::sql_user_lock_wrong_name("too-long-name");
        assert!(lock_zh.contains("too-long-name"));
        set_locale("en-US");
        let invalid_en = messages::sql_user_lock_invalid_name("NULL");
        assert!(invalid_en.contains("NULL"));
        assert!(invalid_en.to_ascii_lowercase().contains("lock"));
        set_locale("zh-CN");
        let invalid_zh = messages::sql_user_lock_invalid_name("NULL");
        assert!(invalid_zh.contains("NULL"));
        set_locale("en-US");
        let binlog_en = messages::sql_binlog_log_not_found("binlog.000001");
        assert!(binlog_en.contains("binlog.000001"));
        set_locale("zh-CN");
        let binlog_zh = messages::sql_binlog_log_not_found("binlog.000001");
        assert!(binlog_zh.contains("binlog.000001"));
        set_locale("en-US");
        let engine_en = messages::sql_unknown_storage_engine("MUSQL");
        assert!(engine_en.contains("MUSQL"));
        assert!(engine_en.to_ascii_lowercase().contains("storage engine"));
        let stub_en = messages::sql_show_engine_innodb_status_stub();
        assert!(!stub_en.is_empty());
        assert!(stub_en.to_ascii_lowercase().contains("stub"));
        set_locale("zh-CN");
        let engine_zh = messages::sql_unknown_storage_engine("MUSQL");
        assert!(engine_zh.contains("MUSQL"));
        let stub_zh = messages::sql_show_engine_innodb_status_stub();
        assert!(!stub_zh.is_empty());
        set_locale("en-US");
        let wrong_en = messages::sql_wrong_object("rusql.gap_v", "VIEW");
        assert!(wrong_en.contains("rusql.gap_v"));
        assert!(wrong_en.contains("VIEW"));
        set_locale("zh-CN");
        let wrong_zh = messages::sql_wrong_object("rusql.gap_v", "VIEW");
        assert!(wrong_zh.contains("rusql.gap_v"));
        assert!(wrong_zh.contains("VIEW"));
        set_locale("en-US");
        let ps_en = messages::sql_unknown_prepared_statement("gap_stmt", "EXECUTE");
        assert!(ps_en.contains("gap_stmt"));
        assert!(ps_en.contains("EXECUTE"));
        set_locale("zh-CN");
        let ps_zh = messages::sql_unknown_prepared_statement("gap_stmt", "EXECUTE");
        assert!(ps_zh.contains("gap_stmt"));
        assert!(ps_zh.contains("EXECUTE"));
        set_locale("en-US");
        assert!(messages::sql_execute_using_unsupported()
            .to_ascii_lowercase()
            .contains("using"));
        assert!(messages::sql_prepare_nested_unsupported()
            .to_ascii_lowercase()
            .contains("prepare"));
        set_locale("en-US");
        let sp_en = messages::sql_savepoint_does_not_exist("gap_sp1");
        assert!(sp_en.contains("gap_sp1"));
        assert!(sp_en.to_ascii_uppercase().contains("SAVEPOINT"));
        set_locale("zh-CN");
        let sp_zh = messages::sql_savepoint_does_not_exist("gap_sp1");
        assert!(sp_zh.contains("gap_sp1"));
        set_locale("en-US");
        let cte_en = messages::sql_cte_max_recursion_depth(1000);
        assert!(cte_en.contains("1000"));
        assert!(cte_en.to_ascii_lowercase().contains("recursive"));
        set_locale("zh-CN");
        let cte_zh = messages::sql_cte_max_recursion_depth(1000);
        assert!(cte_zh.contains("1000"));
        set_locale("en-US");
        let mismatch_en = messages::sql_set_op_column_count_mismatch();
        assert!(mismatch_en.to_ascii_lowercase().contains("column"));
        set_locale("zh-CN");
        let mismatch_zh = messages::sql_set_op_column_count_mismatch();
        assert!(!mismatch_zh.is_empty());
        set_locale("en-US");
        let op_en = messages::sql_unsupported_set_operator("EXCEPT");
        assert!(op_en.contains("EXCEPT"));
        set_locale("zh-CN");
        let op_zh = messages::sql_unsupported_set_operator("EXCEPT");
        assert!(op_zh.contains("EXCEPT"));
        set_locale("en-US");
        assert!(messages::sql_intersect_all_unsupported()
            .to_ascii_uppercase()
            .contains("INTERSECT ALL"));
        set_locale("zh-CN");
        assert!(messages::sql_intersect_all_unsupported()
            .to_ascii_uppercase()
            .contains("INTERSECT ALL"));
        set_locale("en-US");
        let frame_en = messages::sql_window_frame_unsupported();
        assert!(frame_en.to_ascii_uppercase().contains("RANGE") || frame_en.contains("GROUPS"));
        let illegal_en = messages::sql_window_frame_illegal();
        assert!(illegal_en.to_ascii_lowercase().contains("frame"));
        let offset_en = messages::sql_window_frame_offset_invalid();
        assert!(offset_en.to_ascii_lowercase().contains("integer"));
        set_locale("zh-CN");
        let frame_zh = messages::sql_window_frame_unsupported();
        assert!(frame_zh.contains("RANGE") || frame_zh.contains("框架"));
        let illegal_zh = messages::sql_window_frame_illegal();
        assert!(!illegal_zh.is_empty());
        let offset_zh = messages::sql_window_frame_offset_invalid();
        assert!(!offset_zh.is_empty());
        set_locale("en-US");
    }

    #[test]
    fn normalize_locale_aliases() {
        assert_eq!(normalize_locale("zh"), "zh-CN");
        assert_eq!(normalize_locale("en"), "en-US");
    }
}
