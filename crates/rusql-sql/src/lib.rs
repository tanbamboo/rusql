//! SQL parsing for rusql using sqlparser MySQL dialect.

mod bind;
mod grants;
mod show_grants;
mod show_index;
mod show_processlist;
mod stored_programs;

use grants::rewrite_grant_objects;
use show_grants::{
    rewrite_mysql_account_literals, rewrite_show_grants, rewrite_show_grants_current,
};
use show_index::rewrite_show_index;
use show_processlist::rewrite_show_processlist;
use sqlparser::ast::Statement;
use sqlparser::dialect::MySqlDialect;
use sqlparser::parser::Parser;

pub use bind::{bind_placeholders, count_placeholders};
pub use stored_programs::{
    procedure_meta_from_stmt, trigger_meta_from_stmt, try_parse_stored_program, StoredProgramStmt,
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
    let normalized = rewrite_mysql_account_literals(sql);
    let normalized = rewrite_grant_objects(&normalized);
    let rewritten = rewrite_show_index(&normalized);
    let sql = rewritten.as_deref().unwrap_or(&normalized);
    Parser::parse_sql(&MySqlDialect {}, sql).map_err(SqlError::from_parse_err)
}

/// Parse SQL for a connected session (handles `SHOW GRANTS` without `FOR`).
pub fn parse_for_session(sql: &str, user: &str, host: &str) -> Result<Vec<Statement>, SqlError> {
    if let Some(rewritten) = rewrite_show_grants_current(sql, user, host) {
        return Parser::parse_sql(&MySqlDialect {}, &rewritten).map_err(SqlError::from_parse_err);
    }
    parse(sql)
}

pub use show_grants::parse_show_grants;
pub use show_index::parse_show_index_table;

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::ast::Statement;

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
}
