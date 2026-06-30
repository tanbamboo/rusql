//! SQL parsing for rusql using sqlparser MySQL dialect.

use sqlparser::dialect::MySqlDialect;
use sqlparser::parser::Parser;
use sqlparser::ast::Statement;

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
    Parser::parse_sql(&MySqlDialect {}, sql).map_err(SqlError::from_parse_err)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
