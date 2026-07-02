//! SQL parsing for rusql using sqlparser MySQL dialect.

mod bind;
mod show_index;

use show_index::rewrite_show_index;
use sqlparser::ast::Statement;
use sqlparser::dialect::MySqlDialect;
use sqlparser::parser::Parser;

pub use bind::{bind_placeholders, count_placeholders};

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
    let rewritten = rewrite_show_index(sql);
    let sql = rewritten.as_deref().unwrap_or(sql);
    Parser::parse_sql(&MySqlDialect {}, sql).map_err(SqlError::from_parse_err)
}

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
}
