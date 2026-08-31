//! GRANT / REVOKE / SHOW GRANTS execution.

use rusql_core::{Account, GrantTarget, Privilege, PrivilegeStore, Session, DEFAULT_SCHEMA};
use sqlparser::ast::{
    Action, FromTable, GrantObjects, Ident, ObjectName, Privileges, Statement, TableFactor,
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
    session: &Session,
    stmt: &Statement,
) -> Result<(), ExecError> {
    let account = session_account(session);
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
                if !store.has_privilege(&account, &schema, table.as_deref(), required) {
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
            if !store.has_privilege(&account, &schema, Some(&table), Privilege::Insert) {
                return Err(denied_error(
                    &account,
                    Some(&table),
                    Some("INSERT"),
                    "INSERT",
                ));
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
            if !store.has_privilege(&account, &schema, Some(&table), Privilege::Update) {
                return Err(denied_error(
                    &account,
                    Some(&table),
                    Some("UPDATE"),
                    "UPDATE",
                ));
            }
        }
        Statement::Delete(delete) => {
            let table_name = delete_table_name(delete)?;
            let (schema, table) = object_target(session, &table_name)?;
            if !store.has_privilege(&account, &schema, Some(&table), Privilege::Delete) {
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
            if !store.has_privilege(&account, &session.database, None, Privilege::Create) {
                return Err(denied_error(&account, None, Some("CREATE"), "CREATE"));
            }
        }
        Statement::Drop { .. } => {
            if !store.has_privilege(&account, &session.database, None, Privilege::Drop) {
                return Err(denied_error(&account, None, Some("DROP"), "DROP"));
            }
        }
        Statement::AlterTable { .. } | Statement::AlterIndex { .. } => {
            if !store.has_privilege(&account, &session.database, None, Privilege::Alter) {
                return Err(denied_error(&account, None, Some("ALTER"), "ALTER"));
            }
        }
        Statement::CreateIndex { .. } => {
            if !store.has_privilege(&account, &session.database, None, Privilege::Index) {
                return Err(denied_error(&account, None, Some("INDEX"), "INDEX"));
            }
        }
        Statement::Explain { .. }
        | Statement::ExplainTable { .. }
        | Statement::ShowColumns { .. }
        | Statement::ShowCreate { .. }
        | Statement::ShowTables { .. }
        | Statement::ShowDatabases { .. }
        | Statement::Use(_)
        | Statement::StartTransaction { .. }
        | Statement::Commit { .. }
        | Statement::Rollback { .. } => {}
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

fn delete_table_name(delete: &sqlparser::ast::Delete) -> Result<ObjectName, ExecError> {
    let tables = match &delete.from {
        FromTable::WithFromKeyword(t) | FromTable::WithoutKeyword(t) => t,
    };
    let first = tables
        .first()
        .ok_or_else(|| ExecError::Message("DELETE requires a table".into()))?;
    match &first.relation {
        TableFactor::Table { name, .. } => Ok(name.clone()),
        other => Err(ExecError::Message(format!(
            "unsupported DELETE target: {other:?}"
        ))),
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

fn query_privilege(query: &sqlparser::ast::Query) -> Option<Privilege> {
    use sqlparser::ast::SetExpr;
    match query.body.as_ref() {
        SetExpr::Select(select) => {
            if let Some(from) = select.from.first() {
                if let sqlparser::ast::TableFactor::Table { name, .. } = &from.relation {
                    let table = name.0.last()?.value.as_str();
                    if table == SHOW_GRANTS_VIRTUAL_TABLE || table == MYSQL_USER_VIRTUAL_TABLE {
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
