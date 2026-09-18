//! SQL parsing for rusql using sqlparser MySQL dialect.

mod bind;
mod grants;
mod lock_in_share_mode;
mod set_charset;
mod set_global;
mod set_transaction;
mod show_character_set;
mod show_create_database;
mod show_create_function;
mod show_create_procedure;
mod show_create_trigger;
mod show_engines;
mod show_function_status;
mod show_grants;
mod show_index;
mod show_procedure_status;
mod show_processlist;
mod show_table_status;
mod show_triggers;
mod show_warnings;
mod sql_calc_found_rows;
mod stored_programs;
mod user_var_assign;

use grants::rewrite_grant_objects;
use lock_in_share_mode::rewrite_lock_in_share_mode;
use set_charset::rewrite_set_charset;
use set_global::rewrite_set_global;
use set_transaction::rewrite_set_transaction;
use show_character_set::rewrite_show_character_set;
use show_create_database::rewrite_show_create_database;
use show_create_function::rewrite_show_create_function;
use show_create_procedure::rewrite_show_create_procedure;
use show_create_trigger::rewrite_show_create_trigger;
use show_engines::rewrite_show_engines;
use show_function_status::rewrite_show_function_status;
use show_grants::{
    rewrite_mysql_account_literals, rewrite_show_grants, rewrite_show_grants_current,
};
use show_index::rewrite_show_index;
use show_procedure_status::rewrite_show_procedure_status;
use show_processlist::rewrite_show_processlist;
use show_table_status::rewrite_show_table_status;
use show_triggers::rewrite_show_triggers;
use show_warnings::rewrite_show_warnings;
use sql_calc_found_rows::rewrite_sql_calc_found_rows;
use sqlparser::ast::Statement;
use sqlparser::dialect::MySqlDialect;
use sqlparser::parser::Parser;
use user_var_assign::rewrite_user_var_assign;

pub use bind::{bind_placeholders, count_placeholders};
pub use stored_programs::{
    function_meta_from_stmt, procedure_meta_from_stmt, trigger_meta_from_stmt,
    try_parse_stored_program, StoredProgramStmt,
};

/// SQL parse errors.
#[derive(Debug, thiserror::Error)]
pub enum SqlError {
    #[error("{0}")]
    Parse(String),
}

impl SqlError {
    fn from_parse_err(err: sqlparser::parser::ParserError) -> Self {
        Self::Parse(rusql_i18n::messages::sql_parse_error(&err.to_string()))
    }
}

/// Parse a SQL string into AST statements (MySQL dialect).
pub fn parse(sql: &str) -> Result<Vec<Statement>, SqlError> {
    if let Some(rewritten) = rewrite_show_grants(sql) {
        return Parser::parse_sql(&MySqlDialect {}, &rewritten).map_err(SqlError::from_parse_err);
    }
    if let Some(rewritten) = rewrite_show_processlist(sql) {
        return Parser::parse_sql(&MySqlDialect {}, &rewritten).map_err(SqlError::from_parse_err);
    }
    if let Some(rewritten) = rewrite_show_table_status(sql) {
        return Parser::parse_sql(&MySqlDialect {}, &rewritten).map_err(SqlError::from_parse_err);
    }
    if let Some(rewritten) = rewrite_show_triggers(sql) {
        return Parser::parse_sql(&MySqlDialect {}, &rewritten).map_err(SqlError::from_parse_err);
    }
    if let Some(rewritten) = rewrite_show_procedure_status(sql) {
        return Parser::parse_sql(&MySqlDialect {}, &rewritten).map_err(SqlError::from_parse_err);
    }
    if let Some(rewritten) = rewrite_show_function_status(sql) {
        return Parser::parse_sql(&MySqlDialect {}, &rewritten).map_err(SqlError::from_parse_err);
    }
    if let Some(rewritten) = rewrite_show_engines(sql) {
        return Parser::parse_sql(&MySqlDialect {}, &rewritten).map_err(SqlError::from_parse_err);
    }
    if let Some(rewritten) = rewrite_show_character_set(sql) {
        return Parser::parse_sql(&MySqlDialect {}, &rewritten).map_err(SqlError::from_parse_err);
    }
    if let Some(rewritten) = rewrite_show_warnings(sql) {
        return Parser::parse_sql(&MySqlDialect {}, &rewritten).map_err(SqlError::from_parse_err);
    }
    if let Some(rewritten) = rewrite_show_create_database(sql) {
        return Parser::parse_sql(&MySqlDialect {}, &rewritten).map_err(SqlError::from_parse_err);
    }
    if let Some(rewritten) = rewrite_show_create_trigger(sql) {
        return Parser::parse_sql(&MySqlDialect {}, &rewritten).map_err(SqlError::from_parse_err);
    }
    if let Some(rewritten) = rewrite_show_create_procedure(sql) {
        return Parser::parse_sql(&MySqlDialect {}, &rewritten).map_err(SqlError::from_parse_err);
    }
    if let Some(rewritten) = rewrite_show_create_function(sql) {
        return Parser::parse_sql(&MySqlDialect {}, &rewritten).map_err(SqlError::from_parse_err);
    }
    let normalized = rewrite_mysql_account_literals(sql);
    let normalized = rewrite_grant_objects(&normalized);
    let rewritten = rewrite_show_index(&normalized);
    let sql = rewritten.as_deref().unwrap_or(&normalized);
    let sql = rewrite_sql_calc_found_rows(sql).unwrap_or_else(|| sql.to_string());
    let sql = rewrite_set_charset(&sql).unwrap_or(sql);
    let sql = rewrite_user_var_assign(&sql).unwrap_or(sql);
    let sql = rewrite_set_global(&sql).unwrap_or(sql);
    let sql = rewrite_set_transaction(&sql).unwrap_or(sql);
    let sql = rewrite_lock_in_share_mode(&sql).unwrap_or(sql);
    Parser::parse_sql(&MySqlDialect {}, &sql).map_err(SqlError::from_parse_err)
}

/// Parse SQL for a connected session (handles `SHOW GRANTS` without `FOR`).
pub fn parse_for_session(sql: &str, user: &str, host: &str) -> Result<Vec<Statement>, SqlError> {
    if let Some(rewritten) = rewrite_show_grants_current(sql, user, host) {
        return Parser::parse_sql(&MySqlDialect {}, &rewritten).map_err(SqlError::from_parse_err);
    }
    parse(sql)
}

pub use show_character_set::{
    parse_show_character_set, ShowCharacterSet, CHARACTER_SET_VIRTUAL_TABLE,
};
pub use show_create_database::{
    parse_show_create_database, ShowCreateDatabase, CREATE_DATABASE_VIRTUAL_TABLE,
};
pub use show_create_function::{
    parse_show_create_function, ShowCreateFunction, CREATE_FUNCTION_VIRTUAL_TABLE,
};
pub use show_create_procedure::{
    parse_show_create_procedure, ShowCreateProcedure, CREATE_PROCEDURE_VIRTUAL_TABLE,
};
pub use show_create_trigger::{
    parse_show_create_trigger, ShowCreateTrigger, CREATE_TRIGGER_VIRTUAL_TABLE,
};
pub use show_engines::{parse_show_engines, ENGINES_VIRTUAL_TABLE};
pub use show_function_status::{
    parse_show_function_status, ShowFunctionStatus, FUNCTION_STATUS_VIRTUAL_TABLE,
};
pub use show_grants::parse_show_grants;
pub use show_index::parse_show_index_table;
pub use show_procedure_status::{
    parse_show_procedure_status, ShowProcedureStatus, PROCEDURE_STATUS_VIRTUAL_TABLE,
};
pub use show_table_status::{parse_show_table_status, ShowTableStatus, TABLE_STATUS_VIRTUAL_TABLE};
pub use show_triggers::{parse_show_triggers, ShowTriggers, TRIGGERS_VIRTUAL_TABLE};
pub use show_warnings::{parse_show_warnings, WARNINGS_VIRTUAL_TABLE};
pub use sql_calc_found_rows::SQL_CALC_FOUND_ROWS_CTE;
pub use user_var_assign::USER_VAR_ASSIGN_FN;

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::ast::{ShowCreateObject, ShowStatementFilter, Statement};

    #[test]
    fn parse_create_table() {
        let stmts = parse("CREATE TABLE t (id INT PRIMARY KEY)").unwrap();
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn parse_select() {
        let stmts = parse("SELECT 1").unwrap();
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn parse_create_index() {
        let stmts = parse("CREATE INDEX idx ON t (id)").unwrap();
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn parse_show_tables() {
        let stmts = parse("SHOW TABLES").unwrap();
        assert!(matches!(stmts[0], Statement::ShowTables { .. }));
    }

    #[test]
    fn parse_show_databases() {
        let stmts = parse("SHOW DATABASES").unwrap();
        assert!(matches!(stmts[0], Statement::ShowDatabases { .. }));
    }

    #[test]
    fn parse_show_create_table() {
        let stmts = parse("SHOW CREATE TABLE users").unwrap();
        assert!(matches!(stmts[0], Statement::ShowCreate { .. }));
    }

    #[test]
    fn parse_show_create_view() {
        let stmts = parse("SHOW CREATE VIEW v_ids").unwrap();
        match &stmts[0] {
            Statement::ShowCreate { obj_type, obj_name } => {
                assert_eq!(*obj_type, ShowCreateObject::View);
                assert_eq!(obj_name.0.last().unwrap().value, "v_ids");
            }
            other => panic!("expected SHOW CREATE VIEW, got {other:?}"),
        }
        let table = parse("SHOW CREATE TABLE users").unwrap();
        match &table[0] {
            Statement::ShowCreate { obj_type, .. } => {
                assert_eq!(*obj_type, ShowCreateObject::Table);
            }
            other => panic!("SHOW CREATE TABLE must stay ShowCreate Table, got {other:?}"),
        }
        let db = parse("SHOW CREATE DATABASE rusql").unwrap();
        match &db[0] {
            Statement::Query(_) => {}
            other => panic!("SHOW CREATE DATABASE must stay rewritten Query, got {other:?}"),
        }
    }

    #[test]
    fn parse_show_index() {
        let stmts = parse("SHOW INDEX FROM users").unwrap();
        assert_eq!(stmts.len(), 1);
        assert!(matches!(stmts[0], Statement::Query(_)));
    }

    #[test]
    fn parse_use_database() {
        let stmts = parse("USE rusql").unwrap();
        assert!(matches!(stmts[0], Statement::Use(_)));
    }

    #[test]
    fn parse_grant_mysql() {
        let stmts = parse("GRANT SELECT, INSERT ON rusql.* TO app").unwrap();
        assert!(matches!(stmts[0], Statement::Grant { .. }));
    }

    #[test]
    fn parse_revoke_mysql() {
        let stmts = parse("REVOKE INSERT ON rusql.* FROM app").unwrap();
        assert!(matches!(stmts[0], Statement::Revoke { .. }));
    }

    #[test]
    fn parse_insert_select() {
        let stmts = parse("INSERT INTO dst (id, name) SELECT id, name FROM src").unwrap();
        assert!(matches!(stmts[0], Statement::Insert(_)));
    }

    #[test]
    fn parse_insert_on_duplicate_key_update() {
        let stmts =
            parse("INSERT INTO t VALUES (1, 'a') ON DUPLICATE KEY UPDATE name = VALUES(name)")
                .unwrap();
        match &stmts[0] {
            Statement::Insert(insert) => {
                assert!(insert.on.is_some());
            }
            other => panic!("expected Insert, got {other:?}"),
        }
    }

    #[test]
    fn parse_with_cte() {
        let stmts = parse("WITH c AS (SELECT 1 AS id) SELECT id FROM c").unwrap();
        match &stmts[0] {
            Statement::Query(q) => assert!(q.with.is_some()),
            other => panic!("expected Query, got {other:?}"),
        }
    }

    #[test]
    fn parse_session_system_variable() {
        let stmts = parse("SELECT @@version, @@session.autocommit").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected Query, got {other:?}"),
        }
    }

    #[test]
    fn parse_show_variables() {
        let stmts = parse("SHOW VARIABLES").unwrap();
        match &stmts[0] {
            Statement::ShowVariables {
                filter: None,
                session: false,
                global: false,
            } => {}
            other => panic!("expected SHOW VARIABLES, got {other:?}"),
        }

        let stmts = parse("SHOW SESSION VARIABLES").unwrap();
        match &stmts[0] {
            Statement::ShowVariables {
                session: true,
                global: false,
                ..
            } => {}
            other => panic!("expected SHOW SESSION VARIABLES, got {other:?}"),
        }

        let stmts = parse("SHOW GLOBAL VARIABLES").unwrap();
        match &stmts[0] {
            Statement::ShowVariables {
                global: true,
                session: false,
                ..
            } => {}
            other => panic!("expected SHOW GLOBAL VARIABLES, got {other:?}"),
        }

        let stmts = parse("SHOW VARIABLES LIKE 'auto_increment%'").unwrap();
        match &stmts[0] {
            Statement::ShowVariables {
                filter: Some(ShowStatementFilter::Like(pattern)),
                ..
            } => assert_eq!(pattern, "auto_increment%"),
            other => panic!("expected SHOW VARIABLES LIKE, got {other:?}"),
        }
    }

    #[test]
    fn parse_show_table_status_rewrite() {
        let stmts = parse("SHOW TABLE STATUS").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected rewritten SHOW TABLE STATUS query, got {other:?}"),
        }

        let stmts = parse("SHOW TABLE STATUS LIKE 't%'").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected rewritten SHOW TABLE STATUS LIKE query, got {other:?}"),
        }

        assert!(parse_show_table_status("SHOW STATUS").is_none());
        let status = parse("SHOW STATUS").unwrap();
        match &status[0] {
            Statement::ShowStatus { .. } => {}
            other => panic!("SHOW STATUS must stay ShowStatus, got {other:?}"),
        }
    }

    #[test]
    fn parse_show_engines_rewrite() {
        let stmts = parse("SHOW ENGINES").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected rewritten SHOW ENGINES query, got {other:?}"),
        }

        let stmts = parse("SHOW STORAGE ENGINES").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected rewritten SHOW STORAGE ENGINES query, got {other:?}"),
        }

        assert!(parse_show_engines("SHOW ENGINE INNODB STATUS").is_none());
        assert!(parse_show_engines("SHOW TABLE STATUS").is_none());
        let status = parse("SHOW STATUS").unwrap();
        match &status[0] {
            Statement::ShowStatus { .. } => {}
            other => panic!("SHOW STATUS must stay ShowStatus, got {other:?}"),
        }
        let table_status = parse("SHOW TABLE STATUS").unwrap();
        match &table_status[0] {
            Statement::Query(_) => {}
            other => panic!("SHOW TABLE STATUS must stay rewritten Query, got {other:?}"),
        }
    }

    #[test]
    fn parse_show_character_set_rewrite() {
        let stmts = parse("SHOW CHARACTER SET").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected rewritten SHOW CHARACTER SET query, got {other:?}"),
        }
        let stmts = parse("SHOW CHARSET LIKE 'utf8%'").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected rewritten SHOW CHARSET LIKE query, got {other:?}"),
        }
        assert!(parse_show_character_set("SET CHARACTER SET utf8mb4").is_none());
        assert!(parse_show_character_set("SHOW COLLATION").is_none());
        assert!(parse_show_character_set("SHOW ENGINES").is_none());
        let engines = parse("SHOW ENGINES").unwrap();
        match &engines[0] {
            Statement::Query(_) => {}
            other => panic!("SHOW ENGINES must stay rewritten Query, got {other:?}"),
        }
        let collation = parse("SHOW COLLATION").unwrap();
        match &collation[0] {
            Statement::ShowCollation { .. } => {}
            other => panic!("SHOW COLLATION must stay ShowCollation, got {other:?}"),
        }
    }

    #[test]
    fn parse_show_warnings_rewrite() {
        let stmts = parse("SHOW WARNINGS").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected rewritten SHOW WARNINGS query, got {other:?}"),
        }
        let stmts = parse("SHOW ERRORS").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected rewritten SHOW ERRORS query, got {other:?}"),
        }
        assert!(parse_show_warnings("SHOW COUNT(*) WARNINGS").is_none());
        assert!(parse_show_warnings("SHOW WARNINGS LIMIT 1").is_none());
        assert!(parse_show_warnings("SHOW CHARACTER SET").is_none());
        assert!(parse_show_warnings("SHOW ENGINES").is_none());
        let charset = parse("SHOW CHARACTER SET").unwrap();
        match &charset[0] {
            Statement::Query(_) => {}
            other => panic!("SHOW CHARACTER SET must stay rewritten Query, got {other:?}"),
        }
        let engines = parse("SHOW ENGINES").unwrap();
        match &engines[0] {
            Statement::Query(_) => {}
            other => panic!("SHOW ENGINES must stay rewritten Query, got {other:?}"),
        }
    }

    #[test]
    fn parse_show_create_database_rewrite() {
        let stmts = parse("SHOW CREATE DATABASE rusql").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected rewritten SHOW CREATE DATABASE query, got {other:?}"),
        }
        let stmts = parse("SHOW CREATE SCHEMA `app_db`").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected rewritten SHOW CREATE SCHEMA query, got {other:?}"),
        }
        assert!(parse_show_create_database("SHOW CREATE TABLE users").is_none());
        assert!(parse_show_create_database("SHOW CREATE DATABASE IF NOT EXISTS rusql").is_none());
        let table = parse("SHOW CREATE TABLE users").unwrap();
        match &table[0] {
            Statement::ShowCreate { .. } => {}
            other => panic!("SHOW CREATE TABLE must stay ShowCreate, got {other:?}"),
        }
        let warnings = parse("SHOW WARNINGS").unwrap();
        match &warnings[0] {
            Statement::Query(_) => {}
            other => panic!("SHOW WARNINGS must stay rewritten Query, got {other:?}"),
        }
    }

    #[test]
    fn parse_show_triggers_rewrite() {
        let stmts = parse("SHOW TRIGGERS").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected rewritten SHOW TRIGGERS query, got {other:?}"),
        }
        let stmts = parse("SHOW TRIGGERS LIKE 'tr%'").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected rewritten SHOW TRIGGERS LIKE query, got {other:?}"),
        }
        let stmts = parse("SHOW TRIGGERS FROM rusql").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected rewritten SHOW TRIGGERS FROM query, got {other:?}"),
        }
        assert!(parse_show_triggers("SHOW CREATE TRIGGER t").is_none());
        assert!(parse_show_triggers("SHOW CREATE VIEW v").is_none());
        assert!(parse_show_triggers("SHOW CREATE TABLE users").is_none());
        assert!(parse_show_triggers("SHOW CREATE DATABASE rusql").is_none());
        let view = parse("SHOW CREATE VIEW v_ids").unwrap();
        match &view[0] {
            Statement::ShowCreate { obj_type, .. } => {
                assert_eq!(*obj_type, ShowCreateObject::View);
            }
            other => panic!("SHOW CREATE VIEW must stay ShowCreate View, got {other:?}"),
        }
        let table = parse("SHOW CREATE TABLE users").unwrap();
        match &table[0] {
            Statement::ShowCreate { obj_type, .. } => {
                assert_eq!(*obj_type, ShowCreateObject::Table);
            }
            other => panic!("SHOW CREATE TABLE must stay ShowCreate Table, got {other:?}"),
        }
        let db = parse("SHOW CREATE DATABASE rusql").unwrap();
        match &db[0] {
            Statement::Query(_) => {}
            other => panic!("SHOW CREATE DATABASE must stay rewritten Query, got {other:?}"),
        }
    }

    #[test]
    fn parse_show_procedure_status_rewrite() {
        let stmts = parse("SHOW PROCEDURE STATUS").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected rewritten SHOW PROCEDURE STATUS query, got {other:?}"),
        }
        let stmts = parse("SHOW PROCEDURE STATUS LIKE 'p%'").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected rewritten SHOW PROCEDURE STATUS LIKE query, got {other:?}"),
        }
        assert!(parse_show_procedure_status("SHOW CREATE PROCEDURE p").is_none());
        assert!(parse_show_procedure_status("SHOW FUNCTION STATUS").is_none());
        assert!(parse_show_procedure_status("SHOW STATUS").is_none());
        let create = parse("SHOW CREATE PROCEDURE p").unwrap();
        match &create[0] {
            Statement::Query(_) => {}
            other => panic!("SHOW CREATE PROCEDURE must stay rewritten Query, got {other:?}"),
        }
        let function = parse("SHOW CREATE FUNCTION f").unwrap();
        match &function[0] {
            Statement::Query(_) => {}
            other => panic!("SHOW CREATE FUNCTION must stay rewritten Query, got {other:?}"),
        }
        let trigger = parse("SHOW CREATE TRIGGER tr_src").unwrap();
        match &trigger[0] {
            Statement::Query(_) => {}
            other => panic!("SHOW CREATE TRIGGER must stay rewritten Query, got {other:?}"),
        }
    }

    #[test]
    fn parse_show_function_status_rewrite() {
        let stmts = parse("SHOW FUNCTION STATUS").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected rewritten SHOW FUNCTION STATUS query, got {other:?}"),
        }
        let stmts = parse("SHOW FUNCTION STATUS LIKE 'f%'").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected rewritten SHOW FUNCTION STATUS LIKE query, got {other:?}"),
        }
        assert!(parse_show_function_status("SHOW CREATE FUNCTION f").is_none());
        assert!(parse_show_function_status("SHOW PROCEDURE STATUS").is_none());
        assert!(parse_show_function_status("SHOW STATUS").is_none());
        let procedure_status = parse("SHOW PROCEDURE STATUS").unwrap();
        match &procedure_status[0] {
            Statement::Query(_) => {}
            other => panic!("SHOW PROCEDURE STATUS must stay rewritten Query, got {other:?}"),
        }
        let create = parse("SHOW CREATE FUNCTION f").unwrap();
        match &create[0] {
            Statement::Query(_) => {}
            other => panic!("SHOW CREATE FUNCTION must stay rewritten Query, got {other:?}"),
        }
        let procedure = parse("SHOW CREATE PROCEDURE p").unwrap();
        match &procedure[0] {
            Statement::Query(_) => {}
            other => panic!("SHOW CREATE PROCEDURE must stay rewritten Query, got {other:?}"),
        }
    }

    #[test]
    fn parse_show_create_trigger_rewrite() {
        let stmts = parse("SHOW CREATE TRIGGER tr_src").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected rewritten SHOW CREATE TRIGGER query, got {other:?}"),
        }
        let stmts = parse("SHOW CREATE TRIGGER rusql.`tr_src`").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => {
                panic!("expected rewritten qualified SHOW CREATE TRIGGER query, got {other:?}")
            }
        }
        assert!(parse_show_create_trigger("SHOW CREATE TABLE users").is_none());
        assert!(parse_show_create_trigger("SHOW CREATE VIEW v").is_none());
        assert!(parse_show_create_trigger("SHOW CREATE DATABASE rusql").is_none());
        assert!(parse_show_create_trigger("SHOW TRIGGERS").is_none());
        let view = parse("SHOW CREATE VIEW v_ids").unwrap();
        match &view[0] {
            Statement::ShowCreate { obj_type, .. } => {
                assert_eq!(*obj_type, ShowCreateObject::View);
            }
            other => panic!("SHOW CREATE VIEW must stay ShowCreate View, got {other:?}"),
        }
        let table = parse("SHOW CREATE TABLE users").unwrap();
        match &table[0] {
            Statement::ShowCreate { obj_type, .. } => {
                assert_eq!(*obj_type, ShowCreateObject::Table);
            }
            other => panic!("SHOW CREATE TABLE must stay ShowCreate Table, got {other:?}"),
        }
        let triggers = parse("SHOW TRIGGERS").unwrap();
        match &triggers[0] {
            Statement::Query(_) => {}
            other => panic!("SHOW TRIGGERS must stay rewritten Query, got {other:?}"),
        }
    }

    #[test]
    fn parse_show_create_procedure_rewrite() {
        let stmts = parse("SHOW CREATE PROCEDURE p").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected rewritten SHOW CREATE PROCEDURE query, got {other:?}"),
        }
        let stmts = parse("SHOW CREATE PROCEDURE rusql.`p`").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => {
                panic!("expected rewritten qualified SHOW CREATE PROCEDURE query, got {other:?}")
            }
        }
        assert!(parse_show_create_procedure("SHOW CREATE TABLE users").is_none());
        assert!(parse_show_create_procedure("SHOW CREATE VIEW v").is_none());
        assert!(parse_show_create_procedure("SHOW CREATE TRIGGER t").is_none());
        assert!(parse_show_create_procedure("SHOW CREATE FUNCTION f").is_none());
        let function = parse("SHOW CREATE FUNCTION f").unwrap();
        match &function[0] {
            Statement::Query(_) => {}
            other => panic!("SHOW CREATE FUNCTION must be rewritten Query, got {other:?}"),
        }
        let trigger = parse("SHOW CREATE TRIGGER tr_src").unwrap();
        match &trigger[0] {
            Statement::Query(_) => {}
            other => panic!("SHOW CREATE TRIGGER must stay rewritten Query, got {other:?}"),
        }
        let view = parse("SHOW CREATE VIEW v_ids").unwrap();
        match &view[0] {
            Statement::ShowCreate { obj_type, .. } => {
                assert_eq!(*obj_type, ShowCreateObject::View);
            }
            other => panic!("SHOW CREATE VIEW must stay ShowCreate View, got {other:?}"),
        }
    }

    #[test]
    fn parse_show_create_function_rewrite() {
        let stmts = parse("SHOW CREATE FUNCTION f").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected rewritten SHOW CREATE FUNCTION query, got {other:?}"),
        }
        let stmts = parse("SHOW CREATE FUNCTION rusql.`f`").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => {
                panic!("expected rewritten qualified SHOW CREATE FUNCTION query, got {other:?}")
            }
        }
        assert!(parse_show_create_function("SHOW CREATE TABLE users").is_none());
        assert!(parse_show_create_function("SHOW CREATE VIEW v").is_none());
        assert!(parse_show_create_function("SHOW CREATE TRIGGER t").is_none());
        assert!(parse_show_create_function("SHOW CREATE PROCEDURE p").is_none());
        let procedure = parse("SHOW CREATE PROCEDURE p").unwrap();
        match &procedure[0] {
            Statement::Query(_) => {}
            other => panic!("SHOW CREATE PROCEDURE must stay rewritten Query, got {other:?}"),
        }
        let trigger = parse("SHOW CREATE TRIGGER tr_src").unwrap();
        match &trigger[0] {
            Statement::Query(_) => {}
            other => panic!("SHOW CREATE TRIGGER must stay rewritten Query, got {other:?}"),
        }
        let view = parse("SHOW CREATE VIEW v_ids").unwrap();
        match &view[0] {
            Statement::ShowCreate { obj_type, .. } => {
                assert_eq!(*obj_type, ShowCreateObject::View);
            }
            other => panic!("SHOW CREATE VIEW must stay ShowCreate View, got {other:?}"),
        }
    }

    #[test]
    fn parse_show_status() {
        let stmts = parse("SHOW STATUS").unwrap();
        match &stmts[0] {
            Statement::ShowStatus {
                filter: None,
                session: false,
                global: false,
            } => {}
            other => panic!("expected SHOW STATUS, got {other:?}"),
        }

        let stmts = parse("SHOW SESSION STATUS").unwrap();
        match &stmts[0] {
            Statement::ShowStatus {
                session: true,
                global: false,
                ..
            } => {}
            other => panic!("expected SHOW SESSION STATUS, got {other:?}"),
        }

        let stmts = parse("SHOW GLOBAL STATUS").unwrap();
        match &stmts[0] {
            Statement::ShowStatus {
                global: true,
                session: false,
                ..
            } => {}
            other => panic!("expected SHOW GLOBAL STATUS, got {other:?}"),
        }

        let stmts = parse("SHOW STATUS LIKE 'Threads%'").unwrap();
        match &stmts[0] {
            Statement::ShowStatus {
                filter: Some(ShowStatementFilter::Like(pattern)),
                ..
            } => assert_eq!(pattern, "Threads%"),
            other => panic!("expected SHOW STATUS LIKE, got {other:?}"),
        }
    }

    #[test]
    fn parse_set_session_and_global() {
        for sql in [
            "SET @@autocommit = 0",
            "SET @@session.autocommit = 1",
            "SET SESSION autocommit = 1",
            "SET autocommit = 0",
        ] {
            let stmts = parse(sql).unwrap();
            match &stmts[0] {
                Statement::SetVariable { .. } => {}
                other => panic!("expected SET variable for {sql}, got {other:?}"),
            }
        }

        let stmts = parse("SET GLOBAL autocommit = 0").unwrap();
        match &stmts[0] {
            Statement::SetVariable { variables, .. } => {
                let rendered = variables.to_string();
                assert!(
                    rendered.to_ascii_lowercase().contains("global"),
                    "SET GLOBAL should parse as @@global, got {rendered}"
                );
            }
            other => panic!("expected SET GLOBAL as SetVariable, got {other:?}"),
        }

        let stmts = parse("SET NAMES utf8mb4").unwrap();
        match &stmts[0] {
            Statement::SetNames { .. } => {}
            other => panic!("expected SET NAMES, got {other:?}"),
        }

        let stmts = parse("SET NAMES DEFAULT").unwrap();
        match &stmts[0] {
            Statement::SetNamesDefault {} => {}
            other => panic!("expected SET NAMES DEFAULT, got {other:?}"),
        }

        for sql in ["SET CHARACTER SET utf8mb4", "SET CHARSET utf8mb4"] {
            let stmts = parse(sql).unwrap();
            match &stmts[0] {
                Statement::SetNames { charset_name, .. } => {
                    assert_eq!(charset_name.to_ascii_lowercase(), "utf8mb4");
                }
                other => panic!("expected SET NAMES alias for {sql}, got {other:?}"),
            }
        }
    }

    #[test]
    fn parse_set_transaction_isolation() {
        for sql in [
            "SET TRANSACTION ISOLATION LEVEL READ COMMITTED",
            "SET SESSION TRANSACTION ISOLATION LEVEL REPEATABLE READ",
            "SET TRANSACTION ISOLATION LEVEL SERIALIZABLE",
            "SET TRANSACTION ISOLATION LEVEL READ UNCOMMITTED",
        ] {
            let stmts = parse(sql).unwrap();
            match &stmts[0] {
                Statement::SetTransaction { .. } => {}
                other => panic!("expected SET TRANSACTION for {sql}, got {other:?}"),
            }
        }

        let stmts = parse("SET GLOBAL TRANSACTION ISOLATION LEVEL READ COMMITTED").unwrap();
        match &stmts[0] {
            Statement::SetVariable { variables, .. } => {
                let rendered = variables.to_string();
                assert!(
                    rendered.to_ascii_lowercase().contains("global"),
                    "SET GLOBAL TRANSACTION should parse as @@global, got {rendered}"
                );
            }
            other => panic!("expected SET GLOBAL TRANSACTION as SetVariable, got {other:?}"),
        }
    }

    #[test]
    fn parse_set_user_variable() {
        let stmts = parse("SET @foo = 1").unwrap();
        match &stmts[0] {
            Statement::SetVariable { variables, .. } => {
                assert!(
                    variables.to_string().contains("@foo") || variables.to_string().contains("foo"),
                    "expected @foo, got {variables}"
                );
            }
            other => panic!("expected SET @foo, got {other:?}"),
        }

        let stmts = parse("SET @foo := 2").unwrap();
        match &stmts[0] {
            Statement::SetVariable { variables, .. } => {
                assert!(
                    variables.to_string().contains("@foo") || variables.to_string().contains("foo"),
                    "expected @foo, got {variables}"
                );
            }
            other => panic!("expected SET @foo :=, got {other:?}"),
        }
    }

    #[test]
    fn parse_select_user_var_assign() {
        let stmts = parse("SELECT @foo := 1").unwrap();
        match &stmts[0] {
            Statement::Query(_) => {}
            other => panic!("expected Query for SELECT @foo := 1, got {other:?}"),
        }
    }

    #[test]
    fn parse_sql_calc_found_rows() {
        let stmts = parse("SELECT SQL_CALC_FOUND_ROWS id FROM t LIMIT 1").unwrap();
        match &stmts[0] {
            Statement::Query(q) => {
                assert!(q.with.is_some());
            }
            other => panic!("expected Query, got {other:?}"),
        }
    }

    #[test]
    fn parse_select_for_update_and_share() {
        use sqlparser::ast::{LockType, NonBlock};

        for sql in [
            "SELECT id FROM t FOR UPDATE",
            "SELECT id FROM t FOR SHARE",
            "SELECT id FROM t LOCK IN SHARE MODE",
            "SELECT id FROM t FOR UPDATE NOWAIT",
            "SELECT id FROM t FOR UPDATE SKIP LOCKED",
            "SELECT id FROM t FOR SHARE NOWAIT",
            "SELECT id FROM t FOR SHARE SKIP LOCKED",
        ] {
            let stmts = parse(sql).unwrap();
            match &stmts[0] {
                Statement::Query(q) => {
                    assert_eq!(q.locks.len(), 1, "expected one lock clause for {sql}");
                }
                other => panic!("expected Query for {sql}, got {other:?}"),
            }
        }

        match &parse("SELECT id FROM t FOR UPDATE").unwrap()[0] {
            Statement::Query(q) => {
                assert_eq!(q.locks[0].lock_type, LockType::Update);
                assert!(q.locks[0].nonblock.is_none());
            }
            other => panic!("expected FOR UPDATE, got {other:?}"),
        }
        match &parse("SELECT id FROM t FOR SHARE").unwrap()[0] {
            Statement::Query(q) => {
                assert_eq!(q.locks[0].lock_type, LockType::Share);
            }
            other => panic!("expected FOR SHARE, got {other:?}"),
        }
        match &parse("SELECT id FROM t LOCK IN SHARE MODE").unwrap()[0] {
            Statement::Query(q) => {
                assert_eq!(q.locks[0].lock_type, LockType::Share);
            }
            other => panic!("expected LOCK IN SHARE MODE as FOR SHARE, got {other:?}"),
        }
        match &parse("SELECT id FROM t FOR UPDATE NOWAIT").unwrap()[0] {
            Statement::Query(q) => {
                assert_eq!(q.locks[0].nonblock, Some(NonBlock::Nowait));
            }
            other => panic!("expected NOWAIT, got {other:?}"),
        }
        match &parse("SELECT id FROM t FOR UPDATE SKIP LOCKED").unwrap()[0] {
            Statement::Query(q) => {
                assert_eq!(q.locks[0].nonblock, Some(NonBlock::SkipLocked));
            }
            other => panic!("expected SKIP LOCKED, got {other:?}"),
        }
    }
}
