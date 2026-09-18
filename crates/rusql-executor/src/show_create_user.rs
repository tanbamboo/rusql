//! Documented `SHOW CREATE USER` stubs (M99).
//!
//! Reconstructs `CREATE USER … IDENTIFIED WITH '{plugin}'` from M55 accounts.
//! Password hashes stay out of the reconstructed cell.

use crate::{ExecError, QueryResult};
use rusql_core::{Account, PrivilegeStore};

pub(crate) const CREATE_USER_VIRTUAL_TABLE: &str = rusql_sql::CREATE_USER_VIRTUAL_TABLE;

/// MySQL `ER_NO_SUCH_USER`.
const ER_NO_SUCH_USER: u16 = 3162;

/// `SHOW CREATE USER 'u'@'h'` reconstructed from the privilege catalog.
pub(crate) fn show_create_user(
    store: &PrivilegeStore,
    user: &str,
    host: &str,
) -> Result<QueryResult, ExecError> {
    let account = store
        .get_account(user, host)
        .ok_or_else(|| ExecError::Mysql {
            code: ER_NO_SUCH_USER,
            message: rusql_i18n::messages::sql_user_not_found(&Account::new(user, host).display()),
        })?;
    let column = format!("CREATE USER for {user}@{host}");
    Ok(QueryResult::Rows {
        columns: vec![column],
        rows: vec![vec![create_user_ddl(
            &account.user,
            &account.host,
            &account.auth_plugin,
        )]],
    })
}

fn create_user_ddl(user: &str, host: &str, plugin: &str) -> String {
    format!(
        "CREATE USER `{}`@`{}` IDENTIFIED WITH '{}'",
        user.replace('`', "``"),
        host.replace('`', "``"),
        plugin.replace('\'', "''")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_core::{Account, AUTH_PLUGIN_NATIVE};

    #[test]
    fn show_create_user_reconstructed_ddl_and_unknown() {
        let mut store = PrivilegeStore::new();
        store
            .create_user(
                &Account::new("app", "%"),
                "secret",
                AUTH_PLUGIN_NATIVE,
                false,
            )
            .unwrap();

        match show_create_user(&store, "app", "%") {
            Ok(QueryResult::Rows { columns, rows }) => {
                assert_eq!(columns, vec!["CREATE USER for app@%".to_string()]);
                assert_eq!(rows.len(), 1);
                assert_eq!(
                    rows[0][0],
                    "CREATE USER `app`@`%` IDENTIFIED WITH 'mysql_native_password'"
                );
                assert!(!rows[0][0].contains("secret"));
                assert!(!rows[0][0].contains("BY "));
                assert!(!rows[0][0].contains("AS "));
            }
            other => panic!("expected rows, got {other:?}"),
        }

        match show_create_user(&store, "no_such", "%") {
            Err(ExecError::Mysql { code, message }) => {
                assert_eq!(code, ER_NO_SUCH_USER);
                assert!(message.contains("no_such"));
            }
            other => panic!("expected errno 3162, got {other:?}"),
        }
    }
}
