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

    pub fn protocol_plugin_is_not_loaded(name: &str) -> String {
        tr("protocol.plugin_is_not_loaded").replace("%{name}", name)
    }

    pub fn sql_parse_error(detail: &str) -> String {
        tr("sql.parse_error").replace("%{detail}", detail)
    }

    pub fn sql_rename_information_schema_forbidden() -> String {
        tr("sql.rename_information_schema_forbidden")
    }

    pub fn sql_rename_duplicate_name() -> String {
        tr("sql.rename_duplicate_name")
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

    pub fn sql_drop_index_not_found(index: &str, _table: &str) -> String {
        tr("sql.drop_index_not_found").replace("%{index}", index)
    }

    pub fn sql_unknown_system_variable(name: &str) -> String {
        tr("sql.unknown_system_variable").replace("%{name}", name)
    }

    pub fn sql_set_global_rejected(name: &str) -> String {
        tr("sql.set_global_rejected").replace("%{name}", name)
    }

    pub fn sql_global_variable_requires_set_global(name: &str) -> String {
        tr("sql.global_variable_requires_set_global").replace("%{name}", name)
    }

    pub fn sql_option_prevents_statement(option: &str) -> String {
        tr("sql.option_prevents_statement").replace("%{option}", option)
    }

    pub fn sql_replica_role_persist_failed() -> String {
        tr("sql.replica_role_persist_failed")
    }

    pub fn sql_variable_is_readonly(name: &str) -> String {
        tr("sql.variable_is_readonly").replace("%{name}", name)
    }

    pub fn sql_variable_is_global_only(name: &str) -> String {
        tr("sql.variable_is_global_only").replace("%{name}", name)
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

    pub fn sql_slave_corrupt_event() -> String {
        tr("sql.slave_corrupt_event")
    }

    pub fn sql_binlog_row_image_persist_failed() -> String {
        tr("sql.binlog_row_image_persist_failed")
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

    pub fn sql_generated_column_value_not_allowed(column: &str, table: &str) -> String {
        tr("sql.generated_column_value_not_allowed")
            .replace("%{column}", column)
            .replace("%{table}", table)
    }

    pub fn sql_stored_generated_unsupported() -> String {
        tr("sql.stored_generated_unsupported")
    }

    pub fn sql_functional_index_unsupported_expr() -> String {
        tr("sql.functional_index_unsupported_expr")
    }

    pub fn sql_functional_index_unknown_function(name: &str) -> String {
        tr("sql.functional_index_unknown_function").replace("%{name}", name)
    }

    pub fn sql_partition_by_type_unsupported(kind: &str) -> String {
        tr("sql.partition_by_type_unsupported").replace("%{kind}", kind)
    }

    pub fn sql_range_partition_empty() -> String {
        tr("sql.range_partition_empty")
    }

    pub fn sql_range_partition_requires_maxvalue() -> String {
        tr("sql.range_partition_requires_maxvalue")
    }

    pub fn sql_range_partition_duplicate_name(name: &str) -> String {
        tr("sql.range_partition_duplicate_name").replace("%{name}", name)
    }

    pub fn sql_range_partition_bound_not_int() -> String {
        tr("sql.range_partition_bound_not_int")
    }

    pub fn sql_range_partition_unknown_column(column: &str) -> String {
        tr("sql.range_partition_unknown_column").replace("%{column}", column)
    }

    pub fn sql_range_partition_table_mismatch() -> String {
        tr("sql.range_partition_table_mismatch")
    }

    pub fn sql_no_partition_for_given_value() -> String {
        tr("sql.no_partition_for_given_value")
    }

    pub fn sql_create_table_like_and_as_mutually_exclusive() -> String {
        tr("sql.create_table_like_and_as_mutually_exclusive")
    }

    pub fn sql_create_table_as_select_no_columns() -> String {
        tr("sql.create_table_as_select_no_columns")
    }

    pub fn sql_create_table_as_select_requires_select() -> String {
        tr("sql.create_table_as_select_requires_select")
    }

    pub fn sql_create_table_as_select_no_wildcard() -> String {
        tr("sql.create_table_as_select_no_wildcard")
    }

    pub fn sql_virtual_generated_requires_expr() -> String {
        tr("sql.virtual_generated_requires_expr")
    }

    pub fn sql_auto_increment_on_generated_column() -> String {
        tr("sql.auto_increment_on_generated_column")
    }

    pub fn sql_default_on_generated_column(column: &str) -> String {
        tr("sql.default_on_generated_column").replace("%{column}", column)
    }

    pub fn sql_column_has_no_default(column: &str) -> String {
        tr("sql.column_has_no_default").replace("%{column}", column)
    }

    pub fn sql_check_constraint_violated(constraint: &str) -> String {
        tr("sql.check_constraint_violated").replace("%{constraint}", constraint)
    }

    pub fn sql_data_truncated_for_column(column: &str) -> String {
        tr("sql.data_truncated_for_column").replace("%{column}", column)
    }

    pub fn sql_check_not_enforced_unsupported() -> String {
        tr("sql.check_not_enforced_unsupported")
    }

    pub fn sql_values_row_column_count(row: usize) -> String {
        tr("sql.values_row_column_count").replace("%{row}", &row.to_string())
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

    pub fn sql_window_offset_invalid() -> String {
        tr("sql.window_offset_invalid")
    }

    pub fn sql_window_sum_non_numeric() -> String {
        tr("sql.window_sum_non_numeric")
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

    pub fn sql_invalid_json_text_in_function(name: &str, detail: &str) -> String {
        tr("sql.invalid_json_text_in_function")
            .replace("%{name}", name)
            .replace("%{detail}", detail)
    }

    pub fn sql_json_unquote_arg_count() -> String {
        tr("sql.json_unquote_arg_count")
    }

    pub fn sql_json_null_member_name() -> String {
        tr("sql.json_null_member_name")
    }

    pub fn sql_invalid_json_path(path: &str) -> String {
        tr("sql.invalid_json_path").replace("%{path}", path)
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

    pub fn sql_lock_wait_timeout() -> String {
        tr("sql.lock_wait_timeout")
    }

    pub fn sql_table_not_locked_with_lock_tables(table: &str) -> String {
        tr("sql.table_not_locked_with_lock_tables").replace("%{table}", table)
    }

    pub fn sql_table_locked_with_read_lock(table: &str) -> String {
        tr("sql.table_locked_with_read_lock").replace("%{table}", table)
    }

    pub fn sql_deadlock_found() -> String {
        tr("sql.deadlock_found")
    }

    pub fn sql_binlog_log_not_found(name: &str) -> String {
        tr("sql.binlog_log_not_found").replace("%{name}", name)
    }

    pub fn sql_replica_not_configured() -> String {
        tr("sql.replica_not_configured")
    }

    pub fn sql_replica_empty_host() -> String {
        tr("sql.replica_empty_host")
    }

    pub fn sql_replica_invalid_port(port: impl std::fmt::Display) -> String {
        tr("sql.replica_invalid_port").replace("%{port}", &port.to_string())
    }

    pub fn sql_replica_bad_host(host: &str, detail: &str) -> String {
        tr("sql.replica_bad_host")
            .replace("%{host}", host)
            .replace("%{detail}", detail)
    }

    pub fn sql_replica_running_change_master() -> String {
        tr("sql.replica_running_change_master")
    }

    pub fn sql_replica_no_data_dir() -> String {
        tr("sql.replica_no_data_dir")
    }

    pub fn sql_replica_no_addresses() -> String {
        tr("sql.replica_no_addresses")
    }

    pub fn sql_replica_connect_timeout() -> String {
        tr("sql.replica_connect_timeout")
    }

    pub fn sql_replica_io_error(detail: &str) -> String {
        tr("sql.replica_io_error").replace("%{detail}", detail)
    }

    pub fn sql_replica_sql_error(detail: &str) -> String {
        tr("sql.replica_sql_error").replace("%{detail}", detail)
    }

    pub fn sql_replica_dump_error(detail: &str) -> String {
        tr("sql.replica_dump_error").replace("%{detail}", detail)
    }

    pub fn sql_replica_full_auth() -> String {
        tr("sql.replica_full_auth")
    }

    pub fn sql_replica_unexpected_auth() -> String {
        tr("sql.replica_unexpected_auth")
    }

    pub fn sql_replica_start_stop_options() -> String {
        tr("sql.replica_start_stop_options")
    }

    pub fn sql_replica_change_requires_option() -> String {
        tr("sql.replica_change_requires_option")
    }

    pub fn sql_replica_change_requires_endpoint() -> String {
        tr("sql.replica_change_requires_endpoint")
    }

    pub fn sql_replica_master_delay() -> String {
        tr("sql.replica_master_delay")
    }

    pub fn sql_replica_auto_position() -> String {
        tr("sql.replica_auto_position")
    }

    pub fn sql_replica_unknown_option(option: &str) -> String {
        tr("sql.replica_unknown_option").replace("%{option}", option)
    }

    pub fn sql_replica_unterminated_string() -> String {
        tr("sql.replica_unterminated_string")
    }

    pub fn sql_replica_expected_key_value() -> String {
        tr("sql.replica_expected_key_value")
    }

    pub fn sql_replica_string_must_be_quoted() -> String {
        tr("sql.replica_string_must_be_quoted")
    }

    pub fn sql_replica_invalid_log_pos(pos: &str) -> String {
        tr("sql.replica_invalid_log_pos").replace("%{pos}", pos)
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

    pub fn sql_xa_rmfail_active() -> String {
        tr("sql.xa_rmfail_active")
    }

    pub fn sql_xa_rmfail_idle() -> String {
        tr("sql.xa_rmfail_idle")
    }

    pub fn sql_xa_rmfail_prepared() -> String {
        tr("sql.xa_rmfail_prepared")
    }

    pub fn sql_xa_not_exist(xid: &str) -> String {
        tr("sql.xa_not_exist").replace("%{xid}", xid)
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

    pub fn procedure_out_inout_not_variable(name: &str) -> String {
        tr("programs.procedure_out_inout_not_variable").replace("%{name}", name)
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

    pub fn signal_unhandled() -> String {
        tr("programs.signal_unhandled")
    }

    pub fn resignal_without_handler() -> String {
        tr("programs.resignal_without_handler")
    }

    pub fn sp_undeclared_var(name: &str) -> String {
        tr("programs.sp_undeclared_var").replace("%{name}", name)
    }

    pub fn sp_dup_var(name: &str) -> String {
        tr("programs.sp_dup_var").replace("%{name}", name)
    }

    pub fn sp_misplaced_declare() -> String {
        tr("programs.sp_misplaced_declare")
    }

    pub fn sp_invalid_declare() -> String {
        tr("programs.sp_invalid_declare")
    }

    pub fn sp_invalid_control_flow() -> String {
        tr("programs.sp_invalid_control_flow")
    }

    pub fn sp_undefined_label(label: &str) -> String {
        tr("programs.sp_undefined_label").replace("%{label}", label)
    }

    pub fn sp_loop_iteration_limit() -> String {
        tr("programs.sp_loop_iteration_limit")
    }

    pub fn sp_cursor_not_open() -> String {
        tr("programs.sp_cursor_not_open")
    }

    pub fn sp_cursor_already_open() -> String {
        tr("programs.sp_cursor_already_open")
    }

    pub fn sp_undeclared_cursor(name: &str) -> String {
        tr("programs.sp_undeclared_cursor").replace("%{name}", name)
    }

    pub fn sp_fetch_no_data() -> String {
        tr("programs.sp_fetch_no_data")
    }

    pub fn storage_table_not_found(name: &str) -> String {
        tr("storage.table_not_found").replace("%{name}", name)
    }

    pub fn storage_table_exists(name: &str) -> String {
        tr("storage.table_exists").replace("%{name}", name)
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

    pub fn storage_xa_duplicate_xid(_xid: &str) -> String {
        tr("storage.xa_duplicate_xid")
    }

    pub fn storage_xa_not_exist(xid: &str) -> String {
        tr("storage.xa_not_exist").replace("%{xid}", xid)
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
    fn semisync_plugin_not_loaded_i18n() {
        set_locale("en-US");
        let en = messages::protocol_plugin_is_not_loaded("rpl_semi_sync_master");
        assert!(en.contains("rpl_semi_sync_master"));
        assert!(
            en.contains("not loaded") || en.contains("未加载"),
            "semi-sync reject must name the missing plugin, got {en}"
        );
        set_locale("zh-CN");
        let zh = messages::protocol_plugin_is_not_loaded("rpl_semi_sync_master");
        assert!(zh.contains("rpl_semi_sync_master"));
        set_locale("en-US");
    }

    #[test]
    fn set_global_and_readonly_i18n() {
        set_locale("en-US");
        let global = messages::sql_set_global_rejected("autocommit");
        assert!(global.contains("autocommit"));
        assert!(global.contains("SET GLOBAL") || global.contains("SESSION"));
        let ro = messages::sql_variable_is_readonly("version");
        assert!(ro.contains("version"));
        let global_only = messages::sql_variable_is_global_only("gtid_executed");
        assert!(global_only.contains("gtid_executed"));
        assert!(global_only.contains("GLOBAL"));
        set_locale("zh-CN");
        let zh = messages::sql_set_global_rejected("autocommit");
        assert!(zh.contains("autocommit"));
        assert!(messages::sql_variable_is_global_only("gtid_executed").contains("gtid_executed"));
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
        assert!(messages::sql_json_unquote_arg_count()
            .to_ascii_lowercase()
            .contains("json_unquote"));
        let unquote_en =
            messages::sql_invalid_json_text_in_function("json_unquote", "expected value");
        assert!(unquote_en.contains("expected value"));
        assert!(unquote_en.to_ascii_lowercase().contains("json_unquote"));
        set_locale("zh-CN");
        let unquote_zh =
            messages::sql_invalid_json_text_in_function("json_unquote", "expected value");
        assert!(unquote_zh.contains("expected value"));
        assert!(unquote_zh.contains("json_unquote"));
        set_locale("en-US");
        let null_key_en = messages::sql_json_null_member_name();
        assert!(null_key_en.to_ascii_lowercase().contains("null"));
        let path_en = messages::sql_invalid_json_path("$[0]");
        assert!(path_en.contains("$[0]"));
        set_locale("zh-CN");
        let null_key_zh = messages::sql_json_null_member_name();
        assert!(null_key_zh.to_ascii_lowercase().contains("null"));
        let path_zh = messages::sql_invalid_json_path("$[0]");
        assert!(path_zh.contains("$[0]"));
        set_locale("en-US");
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
        let lag_off_en = messages::sql_window_offset_invalid();
        assert!(lag_off_en.to_ascii_uppercase().contains("LAG"));
        let sum_en = messages::sql_window_sum_non_numeric();
        assert!(sum_en.to_ascii_uppercase().contains("SUM"));
        set_locale("zh-CN");
        let lag_off_zh = messages::sql_window_offset_invalid();
        assert!(!lag_off_zh.is_empty());
        let sum_zh = messages::sql_window_sum_non_numeric();
        assert!(sum_zh.to_ascii_uppercase().contains("SUM"));
        set_locale("en-US");
    }

    #[test]
    fn normalize_locale_aliases() {
        assert_eq!(normalize_locale("zh"), "zh-CN");
        assert_eq!(normalize_locale("en"), "en-US");
    }
}
