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

    pub fn procedure_exists(name: &str) -> String {
        tr("programs.procedure_exists").replace("%{name}", name)
    }

    pub fn procedure_not_found(name: &str) -> String {
        tr("programs.procedure_not_found").replace("%{name}", name)
    }

    pub fn trigger_exists(name: &str) -> String {
        tr("programs.trigger_exists").replace("%{name}", name)
    }

    pub fn trigger_not_found(name: &str) -> String {
        tr("programs.trigger_not_found").replace("%{name}", name)
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
    fn normalize_locale_aliases() {
        assert_eq!(normalize_locale("zh"), "zh-CN");
        assert_eq!(normalize_locale("en"), "en-US");
    }
}
