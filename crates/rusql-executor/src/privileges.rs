//! GRANT / REVOKE / SHOW GRANTS execution.

use rusql_core::{
    Account, GrantTarget, Privilege, PrivilegeError, PrivilegeStore, Session, DEFAULT_SCHEMA,
};
use rusql_sql::{RoleName, RoleStatement, SetRole};
use sqlparser::ast::{
    Action, GrantObjects, Ident, ObjectName, OnInsert, Privileges, SetExpr, Statement, TableFactor,
};
use std::collections::BTreeSet;

use crate::{ExecError, QueryResult};

pub const SHOW_GRANTS_VIRTUAL_TABLE: &str = "__rusql_show_grants";
pub const MYSQL_USER_VIRTUAL_TABLE: &str = "__rusql_mysql_user";

pub fn execute_grant(
    store: &mut PrivilegeStore,
    session: &Session,
    privileges: &Privileges,
    objects: &GrantObjects,
    grantees: &[Ident],
    with_grant_option: bool,
) -> Result<QueryResult, ExecError> {
    ensure_grant_admin(session)?;
    if with_grant_option {
        return Err(ExecError::Message(
            "WITH GRANT OPTION is not supported".into(),
        ));
    }
    let privs = privileges_to_set(privileges)?;
    for target in grant_targets(objects)? {
        for grantee in grantees {
            let account = parse_account_ident(grantee);
            store.grant(&account, target.clone(), privs.clone());
        }
    }
    Ok(QueryResult::Ok { rows_affected: 0 })
}

pub fn execute_revoke(
    store: &mut PrivilegeStore,
    session: &Session,
    privileges: &Privileges,
    objects: &GrantObjects,
    grantees: &[Ident],
) -> Result<QueryResult, ExecError> {
    ensure_grant_admin(session)?;
    let privs = privileges_to_set(privileges)?;
    for target in grant_targets(objects)? {
        for grantee in grantees {
            let account = parse_account_ident(grantee);
            store.revoke(&account, target.clone(), privs.clone());
        }
    }
    Ok(QueryResult::Ok { rows_affected: 0 })
}

/// Reload `mysql.user.json` into `store`. Disk replaces memory. A read error keeps `store`.
pub fn execute_flush_privileges(
    store: &mut PrivilegeStore,
    session: &Session,
    data_dir: &std::path::Path,
) -> Result<QueryResult, ExecError> {
    if !PrivilegeStore::is_superuser(&session.user) {
        return Err(ExecError::Mysql {
            code: 1227,
            message: rusql_i18n::messages::sql_reload_privilege_required(
                &session.user,
                &session.host,
            ),
        });
    }
    match PrivilegeStore::load(data_dir) {
        Ok(loaded) => {
            *store = loaded;
            Ok(QueryResult::Ok { rows_affected: 0 })
        }
        Err(_) => Err(ExecError::Mysql {
            code: 1105,
            message: rusql_i18n::messages::sql_flush_privileges_reload_failed(),
        }),
    }
}

pub fn execute_role_statement(
    store: &mut PrivilegeStore,
    session: &mut Session,
    stmt: &RoleStatement,
) -> Result<QueryResult, ExecError> {
    match stmt {
        RoleStatement::Create {
            names,
            if_not_exists,
        } => {
            ensure_role_admin(session)?;
            store
                .create_roles(&accounts_from(names), *if_not_exists)
                .map_err(privilege_error)?;
        }
        RoleStatement::Drop { names, if_exists } => {
            ensure_role_admin(session)?;
            store
                .drop_roles(&accounts_from(names), *if_exists)
                .map_err(privilege_error)?;
        }
        RoleStatement::Grant { roles, grantees } => {
            ensure_grant_admin(session)?;
            store
                .grant_roles(&accounts_from(roles), &accounts_from(grantees))
                .map_err(privilege_error)?;
        }
        RoleStatement::Revoke { roles, grantees } => {
            ensure_grant_admin(session)?;
            store
                .revoke_roles(&accounts_from(roles), &accounts_from(grantees))
                .map_err(privilege_error)?;
        }
        RoleStatement::Set(spec) => set_role(store, session, spec)?,
    }
    Ok(QueryResult::Ok { rows_affected: 0 })
}

fn set_role(
    store: &PrivilegeStore,
    session: &mut Session,
    spec: &SetRole,
) -> Result<(), ExecError> {
    let account = session_account(session);
    let active = match spec {
        SetRole::None | SetRole::Default => Vec::new(),
        SetRole::All => store.roles_granted_to(&account),
        SetRole::AllExcept(except) => {
            let excepted = accounts_from(except);
            let mut roles = store.roles_granted_to(&account);
            for role in &excepted {
                if !roles.iter().any(|granted| same_role(granted, role)) {
                    return Err(privilege_error(PrivilegeError::RoleNotGranted {
                        role: role.clone(),
                        grantee: account.clone(),
                    }));
                }
            }
            roles.retain(|granted| !excepted.iter().any(|role| same_role(granted, role)));
            roles
        }
        SetRole::Named(names) => {
            let roles = accounts_from(names);
            for role in &roles {
                if !store.is_role_granted_to(role, &account) {
                    return Err(privilege_error(PrivilegeError::RoleNotGranted {
                        role: role.clone(),
                        grantee: account.clone(),
                    }));
                }
            }
            roles
        }
    };
    session.active_roles = active;
    Ok(())
}

fn accounts_from(names: &[RoleName]) -> Vec<Account> {
    names
        .iter()
        .map(|name| Account::new(name.user.clone(), name.host.clone()))
        .collect()
}

fn same_role(left: &Account, right: &Account) -> bool {
    left.user.eq_ignore_ascii_case(&right.user) && left.host == right.host
}

fn ensure_role_admin(session: &Session) -> Result<(), ExecError> {
    if PrivilegeStore::is_superuser(&session.user) {
        Ok(())
    } else {
        Err(ExecError::Mysql {
            code: 1227,
            message: rusql_i18n::messages::sql_create_role_required(&session.user, &session.host),
        })
    }
}

fn privilege_error(error: PrivilegeError) -> ExecError {
    let (code, message) = match error {
        PrivilegeError::AlreadyExists { operation, account }
        | PrivilegeError::DoesNotExist { operation, account } => (
            1396,
            rusql_i18n::messages::sql_cannot_user(operation, &account.display()),
        ),
        PrivilegeError::UnknownAuthId(account) => (
            3523,
            rusql_i18n::messages::sql_unknown_authid(&account.user, &account.host),
        ),
        PrivilegeError::RoleNotGranted { role, grantee } => (
            3530,
            rusql_i18n::messages::sql_role_not_granted(
                &role.user,
                &role.host,
                &grantee.user,
                &grantee.host,
            ),
        ),
        PrivilegeError::AnonymousGrantee => (
            3876,
            rusql_i18n::messages::sql_cannot_grant_roles_anonymous(),
        ),
    };
    ExecError::Mysql { code, message }
}

pub fn show_grants_result(
    store: &PrivilegeStore,
    user: &str,
    host: &str,
) -> Result<QueryResult, ExecError> {
    let account = Account::new(user, host);
    let column = format!("Grants for {}@{}", account.user, account.host);
    let rows = store
        .show_grants(&account)
        .into_iter()
        .map(|line| vec![line])
        .collect();
    Ok(QueryResult::Rows {
        columns: vec![column],
        rows,
    })
}

pub fn mysql_user_stub_rows(store: &PrivilegeStore) -> QueryResult {
    let rows = store
        .mysql_user_rows()
        .into_iter()
        .map(|(user, host, privs)| vec![user, host, privs])
        .collect();
    QueryResult::Rows {
        columns: vec!["User".into(), "Host".into(), "privileges".into()],
        rows,
    }
}

pub fn check_statement_privilege(
    store: &PrivilegeStore,
    session: &mut Session,
    stmt: &Statement,
) -> Result<(), ExecError> {
    let account = session_account(session);
    session
        .active_roles
        .retain(|role| store.is_role_granted_to(role, &account));
    if PrivilegeStore::is_superuser(&account.user) {
        return Ok(());
    }
    match stmt {
        Statement::Grant { .. } | Statement::Revoke { .. } => {
            return Err(denied_error(&account, None, None, "GRANT"));
        }
        Statement::Query(query) => {
            if let Some(required) = query_privilege(query) {
                let (schema, table) = query_target(session, query)?;
                if !store.has_effective_privilege(
                    &account,
                    &schema,
                    table.as_deref(),
                    required,
                    &session.active_roles,
                ) {
                    return Err(denied_error(
                        &account,
                        table.as_deref(),
                        Some(required.as_str()),
                        required.as_str(),
                    ));
                }
            }
        }
        Statement::Insert(insert) => {
            let (schema, table) = object_target(session, &insert.table_name)?;
            if !store.has_effective_privilege(
                &account,
                &schema,
                Some(&table),
                Privilege::Insert,
                &session.active_roles,
            ) {
                return Err(denied_error(
                    &account,
                    Some(&table),
                    Some("INSERT"),
                    "INSERT",
                ));
            }
            if matches!(&insert.on, Some(OnInsert::DuplicateKeyUpdate(_)))
                && !store.has_effective_privilege(
                    &account,
                    &schema,
                    Some(&table),
                    Privilege::Update,
                    &session.active_roles,
                )
            {
                return Err(denied_error(
                    &account,
                    Some(&table),
                    Some("UPDATE"),
                    "UPDATE",
                ));
            }
            if let Some(source) = insert.source.as_deref() {
                if !matches!(source.body.as_ref(), SetExpr::Values(_)) {
                    if let Some(required) = query_privilege(source) {
                        let (src_schema, src_table) = query_target(session, source)?;
                        if !store.has_effective_privilege(
                            &account,
                            &src_schema,
                            src_table.as_deref(),
                            required,
                            &session.active_roles,
                        ) {
                            return Err(denied_error(
                                &account,
                                src_table.as_deref(),
                                Some(required.as_str()),
                                required.as_str(),
                            ));
                        }
                    }
                }
            }
        }
        Statement::Update { table, .. } => {
            let table_name = match &table.relation {
                TableFactor::Table { name, .. } => name.clone(),
                other => {
                    return Err(ExecError::Message(format!(
                        "unsupported UPDATE target: {other:?}"
                    )))
                }
            };
            let (schema, table) = object_target(session, &table_name)?;
            if !store.has_effective_privilege(
                &account,
                &schema,
                Some(&table),
                Privilege::Update,
                &session.active_roles,
            ) {
                return Err(denied_error(
                    &account,
                    Some(&table),
                    Some("UPDATE"),
                    "UPDATE",
                ));
            }
        }
        Statement::Delete(delete) => {
            let storage_key = crate::multi_table_dml::delete_target_storage_key(session, delete)?;
            let (schema, table) = schema_table_from_storage_key(session, &storage_key)?;
            if !store.has_effective_privilege(
                &account,
                &schema,
                Some(&table),
                Privilege::Delete,
                &session.active_roles,
            ) {
                return Err(denied_error(
                    &account,
                    Some(&table),
                    Some("DELETE"),
                    "DELETE",
                ));
            }
        }
        Statement::CreateTable { .. }
        | Statement::CreateDatabase { .. }
        | Statement::CreateView { .. } => {
            if !store.has_effective_privilege(
                &account,
                &session.database,
                None,
                Privilege::Create,
                &session.active_roles,
            ) {
                return Err(denied_error(&account, None, Some("CREATE"), "CREATE"));
            }
        }
        Statement::Drop { .. } => {
            if !store.has_effective_privilege(
                &account,
                &session.database,
                None,
                Privilege::Drop,
                &session.active_roles,
            ) {
                return Err(denied_error(&account, None, Some("DROP"), "DROP"));
            }
        }
        Statement::Truncate { table_names, .. } => {
            if let Some(target) = table_names.first() {
                let (schema, table) = object_target(session, &target.name)?;
                if !store.has_effective_privilege(
                    &account,
                    &schema,
                    Some(&table),
                    Privilege::Drop,
                    &session.active_roles,
                ) {
                    return Err(denied_error(
                        &account,
                        Some(&table),
                        Some("DROP"),
                        "TRUNCATE",
                    ));
                }
            }
        }
        Statement::AlterTable { .. } | Statement::AlterIndex { .. } => {
            if !store.has_effective_privilege(
                &account,
                &session.database,
                None,
                Privilege::Alter,
                &session.active_roles,
            ) {
                return Err(denied_error(&account, None, Some("ALTER"), "ALTER"));
            }
        }
        Statement::CreateIndex { .. } => {
            if !store.has_effective_privilege(
                &account,
                &session.database,
                None,
                Privilege::Index,
                &session.active_roles,
            ) {
                return Err(denied_error(&account, None, Some("INDEX"), "INDEX"));
            }
        }
        Statement::Explain { .. }
        | Statement::ExplainTable { .. }
        | Statement::ShowColumns { .. }
        | Statement::ShowCreate { .. }
        | Statement::ShowTables { .. }
        | Statement::ShowDatabases { .. }
        | Statement::ShowVariables { .. }
        | Statement::ShowStatus { .. }
        | Statement::SetVariable { .. }
        | Statement::SetNames { .. }
        | Statement::SetNamesDefault {}
        | Statement::SetTransaction { .. }
        | Statement::Use(_)
        | Statement::StartTransaction { .. }
        | Statement::Commit { .. }
        | Statement::Rollback { .. }
        | Statement::Savepoint { .. }
        | Statement::ReleaseSavepoint { .. }
        | Statement::LockTables { .. }
        | Statement::UnlockTables => {}
        other => {
            let _ = other;
        }
    }
    Ok(())
}

fn ensure_grant_admin(session: &Session) -> Result<(), ExecError> {
    if PrivilegeStore::is_superuser(&session.user) {
        Ok(())
    } else {
        Err(denied_error(
            &session_account(session),
            None,
            Some("GRANT"),
            "GRANT",
        ))
    }
}

fn session_account(session: &Session) -> Account {
    Account::new(session.user.clone(), session.host.clone())
}

fn denied_error(
    account: &Account,
    table: Option<&str>,
    _privilege: Option<&str>,
    command: &str,
) -> ExecError {
    let message = if let Some(table) = table {
        rusql_i18n::messages::sql_command_denied_table(command, &account.user, &account.host, table)
    } else {
        rusql_i18n::messages::sql_command_denied(command, &account.user, &account.host)
    };
    ExecError::Mysql {
        code: 1142,
        message,
    }
}

fn privileges_to_set(privileges: &Privileges) -> Result<BTreeSet<Privilege>, ExecError> {
    match privileges {
        Privileges::All { .. } => Ok(BTreeSet::from([
            Privilege::Select,
            Privilege::Insert,
            Privilege::Update,
            Privilege::Delete,
            Privilege::Create,
            Privilege::Drop,
            Privilege::Index,
            Privilege::Alter,
        ])),
        Privileges::Actions(actions) => {
            let mut set = BTreeSet::new();
            for action in actions {
                if let Some(p) = action_to_privilege(action) {
                    set.insert(p);
                }
            }
            if set.is_empty() {
                return Err(ExecError::Message("no supported privileges".into()));
            }
            Ok(set)
        }
    }
}

fn action_to_privilege(action: &Action) -> Option<Privilege> {
    match action {
        Action::Select { .. } => Some(Privilege::Select),
        Action::Insert { .. } => Some(Privilege::Insert),
        Action::Update { .. } => Some(Privilege::Update),
        Action::Delete => Some(Privilege::Delete),
        Action::Create => Some(Privilege::Create),
        _ => None,
    }
}

fn object_name_last(name: &ObjectName) -> String {
    name.0
        .last()
        .map(|ident| ident.value.clone())
        .unwrap_or_default()
}

fn grant_targets(objects: &GrantObjects) -> Result<Vec<GrantTarget>, ExecError> {
    match objects {
        GrantObjects::Tables(names) | GrantObjects::Schemas(names) => {
            names.iter().map(parse_grant_object).collect()
        }
        GrantObjects::AllTablesInSchema { schemas } => schemas
            .iter()
            .map(|name| {
                Ok(GrantTarget {
                    schema: object_name_last(name),
                    table: None,
                })
            })
            .collect(),
        other => Err(ExecError::Message(format!(
            "unsupported GRANT object: {other}"
        ))),
    }
}

fn parse_grant_object(name: &ObjectName) -> Result<GrantTarget, ExecError> {
    let parts: Vec<String> = name.0.iter().map(|ident| ident.value.clone()).collect();
    match parts.as_slice() {
        [schema, star] if star == "*" => Ok(GrantTarget {
            schema: schema.clone(),
            table: None,
        }),
        [schema, table] => Ok(GrantTarget {
            schema: schema.clone(),
            table: Some(table.clone()),
        }),
        [table] => Ok(GrantTarget {
            schema: DEFAULT_SCHEMA.to_string(),
            table: Some(table.clone()),
        }),
        _ => Err(ExecError::Message(format!(
            "unsupported grant object: {name}"
        ))),
    }
}

pub fn parse_account_ident(ident: &Ident) -> Account {
    let value = ident.value.trim_matches('`');
    if let Some((user, host)) = value.split_once('@') {
        Account::new(user.to_string(), host.to_string())
    } else {
        Account::new(value.to_string(), "%")
    }
}

fn object_target(session: &Session, name: &ObjectName) -> Result<(String, String), ExecError> {
    let parts: Vec<String> = name.0.iter().map(|ident| ident.value.clone()).collect();
    match parts.as_slice() {
        [table] => Ok((session.database.clone(), table.clone())),
        [schema, table] => Ok((schema.clone(), table.clone())),
        _ => Err(ExecError::Message(format!("unsupported object: {name}"))),
    }
}

fn schema_table_from_storage_key(
    session: &Session,
    storage_key: &str,
) -> Result<(String, String), ExecError> {
    match storage_key.split('.').collect::<Vec<_>>().as_slice() {
        [schema, table] => Ok(((*schema).to_string(), (*table).to_string())),
        [table] => Ok((session.database.clone(), (*table).to_string())),
        _ => Err(ExecError::Message(format!(
            "unsupported table storage key: {storage_key}"
        ))),
    }
}

fn query_privilege(query: &sqlparser::ast::Query) -> Option<Privilege> {
    use sqlparser::ast::SetExpr;
    match query.body.as_ref() {
        SetExpr::Select(select) => {
            if select.from.is_empty() {
                return None;
            }
            if let Some(from) = select.from.first() {
                if let sqlparser::ast::TableFactor::Table { name, .. } = &from.relation {
                    let table = name.0.last()?.value.as_str();
                    if table == SHOW_GRANTS_VIRTUAL_TABLE
                        || table == MYSQL_USER_VIRTUAL_TABLE
                        || table == crate::show_table_status::TABLE_STATUS_VIRTUAL_TABLE
                        || table == crate::show_engines::ENGINES_VIRTUAL_TABLE
                        || table == crate::show_engine_status::ENGINE_STATUS_VIRTUAL_TABLE
                        || table == crate::show_character_set::CHARACTER_SET_VIRTUAL_TABLE
                        || table == crate::show_warnings::WARNINGS_VIRTUAL_TABLE
                        || table == crate::show_create_database::CREATE_DATABASE_VIRTUAL_TABLE
                        || table == crate::show_create_trigger::CREATE_TRIGGER_VIRTUAL_TABLE
                        || table == crate::show_create_procedure::CREATE_PROCEDURE_VIRTUAL_TABLE
                        || table == crate::show_create_function::CREATE_FUNCTION_VIRTUAL_TABLE
                        || table == crate::show_create_user::CREATE_USER_VIRTUAL_TABLE
                        || table == crate::show_create_event::CREATE_EVENT_VIRTUAL_TABLE
                        || table == crate::show_procedure_status::PROCEDURE_STATUS_VIRTUAL_TABLE
                        || table == crate::show_function_status::FUNCTION_STATUS_VIRTUAL_TABLE
                        || table == crate::show_events::EVENTS_VIRTUAL_TABLE
                        || table == crate::show_triggers::TRIGGERS_VIRTUAL_TABLE
                    {
                        return None;
                    }
                }
            }
            Some(Privilege::Select)
        }
        SetExpr::SetOperation { .. } => Some(Privilege::Select),
        SetExpr::Query(_) => Some(Privilege::Select),
        SetExpr::Insert(_) | SetExpr::Update(_) => None,
        SetExpr::Table(_) => Some(Privilege::Select),
        SetExpr::Values { .. } => Some(Privilege::Select),
    }
}

fn query_target(
    session: &Session,
    query: &sqlparser::ast::Query,
) -> Result<(String, Option<String>), ExecError> {
    use sqlparser::ast::{SetExpr, TableFactor};
    match query.body.as_ref() {
        SetExpr::Select(select) => {
            if let Some(from) = select.from.first() {
                if let TableFactor::Table { name, .. } = &from.relation {
                    let (schema, table) = object_target(session, name)?;
                    return Ok((schema, Some(table)));
                }
            }
            Ok((session.database.clone(), None))
        }
        _ => Ok((session.database.clone(), None)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_core::{GrantTarget, AUTH_PLUGIN_NATIVE};
    use rusql_sql::{parse_role_statement, RoleParse};
    use std::collections::BTreeSet;

    fn role_stmt(sql: &str) -> RoleStatement {
        match parse_role_statement(sql) {
            RoleParse::Statement(stmt) => stmt,
            other => panic!("expected role statement, got {other:?}"),
        }
    }

    fn mysql_code(err: ExecError) -> u16 {
        match err {
            ExecError::Mysql { code, .. } => code,
            other => panic!("expected mysql error, got {other:?}"),
        }
    }

    #[test]
    fn roles_set_role_allows_select_and_revoke_denies() {
        let mut store = PrivilegeStore::new();
        let mut admin = Session::new(1, "root");
        admin.host = "%".into();
        execute_role_statement(&mut store, &mut admin, &role_stmt("CREATE ROLE r")).unwrap();
        store.grant(
            &Account::new("r", "%"),
            GrantTarget {
                schema: "rusql".into(),
                table: None,
            },
            BTreeSet::from([Privilege::Select]),
        );
        store
            .create_user(
                &Account::new("app", "%"),
                "secret",
                AUTH_PLUGIN_NATIVE,
                false,
            )
            .unwrap();
        execute_role_statement(&mut store, &mut admin, &role_stmt("GRANT r TO 'app'@'%'")).unwrap();

        let mut app = Session::new(2, "app");
        app.host = "%".into();
        let select = rusql_sql::parse("SELECT id FROM t").unwrap().remove(0);
        assert_eq!(
            mysql_code(check_statement_privilege(&store, &mut app, &select).unwrap_err()),
            1142
        );
        execute_role_statement(&mut store, &mut app, &role_stmt("SET ROLE r")).unwrap();
        check_statement_privilege(&store, &mut app, &select).unwrap();
        assert_eq!(
            rusql_core::format_current_role(&app.active_roles),
            "`r`@`%`"
        );

        execute_role_statement(
            &mut store,
            &mut admin,
            &role_stmt("REVOKE r FROM 'app'@'%'"),
        )
        .unwrap();
        assert_eq!(
            mysql_code(check_statement_privilege(&store, &mut app, &select).unwrap_err()),
            1142
        );
        assert!(app.active_roles.is_empty());

        assert_eq!(
            mysql_code(
                execute_role_statement(&mut store, &mut admin, &role_stmt("CREATE ROLE r"))
                    .unwrap_err()
            ),
            1396
        );
        assert_eq!(
            mysql_code(
                execute_role_statement(&mut store, &mut app, &role_stmt("SET ROLE missing"))
                    .unwrap_err()
            ),
            3530
        );
        assert_eq!(
            mysql_code(
                execute_role_statement(
                    &mut store,
                    &mut admin,
                    &role_stmt("GRANT nope TO 'app'@'%'")
                )
                .unwrap_err()
            ),
            3523
        );
        assert_eq!(
            mysql_code(
                execute_role_statement(&mut store, &mut app, &role_stmt("CREATE ROLE other"))
                    .unwrap_err()
            ),
            1227
        );
        assert_eq!(
            mysql_code(
                execute_role_statement(&mut store, &mut admin, &role_stmt("GRANT r TO ''@'%'"))
                    .unwrap_err()
            ),
            3876
        );
        execute_role_statement(&mut store, &mut admin, &role_stmt("DROP ROLE r")).unwrap();
        assert_eq!(
            mysql_code(
                execute_role_statement(&mut store, &mut admin, &role_stmt("DROP ROLE r"))
                    .unwrap_err()
            ),
            1396
        );
    }

    fn temp_priv_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rusql-flush-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn account(user: &str) -> Account {
        Account::new(user, "%")
    }

    fn add_user(store: &mut PrivilegeStore, user: &str) {
        store
            .create_user(&account(user), "secret", AUTH_PLUGIN_NATIVE, false)
            .unwrap();
    }

    #[test]
    fn flush_privileges_reloads_disk_over_live_grants() {
        let dir = temp_priv_dir("reload");
        let mut store = PrivilegeStore::new();
        add_user(&mut store, "live");
        store.grant(
            &account("live"),
            GrantTarget {
                schema: "app".into(),
                table: None,
            },
            BTreeSet::from([Privilege::Select]),
        );
        store.save(&dir).unwrap();
        add_user(&mut store, "memory_only");
        assert!(store.resolve_auth("memory_only", "127.0.0.1").is_some());

        let mut disk = PrivilegeStore::new();
        add_user(&mut disk, "fresh");
        disk.save(&dir).unwrap();

        let mut root = Session::new(1, "root");
        root.host = "%".into();
        assert!(matches!(
            execute_flush_privileges(&mut store, &root, &dir).unwrap(),
            QueryResult::Ok { rows_affected: 0 }
        ));
        assert!(store.resolve_auth("fresh", "127.0.0.1").is_some());
        assert!(store.resolve_auth("live", "127.0.0.1").is_none());
        assert!(store.resolve_auth("memory_only", "127.0.0.1").is_none());
        assert!(!store.has_effective_privilege(
            &account("live"),
            "app",
            None,
            Privilege::Select,
            &[],
        ));

        let saved = store.clone();
        std::fs::write(PrivilegeStore::grants_file(&dir), b"{not-json").unwrap();
        assert_eq!(
            mysql_code(execute_flush_privileges(&mut store, &root, &dir).unwrap_err()),
            1105
        );
        assert_eq!(
            store.resolve_auth("fresh", "127.0.0.1").map(|row| row.user),
            saved.resolve_auth("fresh", "127.0.0.1").map(|row| row.user)
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn flush_privileges_denied_for_non_superuser_and_missing_file_is_empty() {
        let dir = temp_priv_dir("deny");
        let mut store = PrivilegeStore::new();
        add_user(&mut store, "keep");
        store.save(&dir).unwrap();
        let mut app = Session::new(2, "app");
        app.host = "%".into();
        assert_eq!(
            mysql_code(execute_flush_privileges(&mut store, &app, &dir).unwrap_err()),
            1227
        );
        assert!(store.resolve_auth("keep", "127.0.0.1").is_some());

        std::fs::remove_file(PrivilegeStore::grants_file(&dir)).unwrap();
        let mut root = Session::new(1, "root");
        root.host = "%".into();
        execute_flush_privileges(&mut store, &root, &dir).unwrap();
        assert!(!store.has_accounts());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
