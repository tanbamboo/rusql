//! Parse generation expressions for VIRTUAL columns (M146).

use sqlparser::ast::{Expr, SelectItem, SetExpr, Statement};
use sqlparser::dialect::MySqlDialect;
use sqlparser::parser::Parser;

use crate::SqlError;

/// Parse a scalar expression as it appears in `AS (expr)`.
pub fn parse_generation_expression(sql: &str) -> Result<Expr, SqlError> {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return Err(SqlError::Parse("empty generation expression".into()));
    }
    let wrapped = format!("SELECT {trimmed}");
    let stmts = Parser::parse_sql(&MySqlDialect {}, &wrapped).map_err(SqlError::from_parse_err)?;
    let Some(Statement::Query(query)) = stmts.into_iter().next() else {
        return Err(SqlError::Parse(
            "generation expression must be scalar".into(),
        ));
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        return Err(SqlError::Parse(
            "generation expression must be scalar".into(),
        ));
    };
    let Some(item) = select.projection.first() else {
        return Err(SqlError::Parse(
            "generation expression must be scalar".into(),
        ));
    };
    let expr = match item {
        SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => expr.clone(),
        _ => {
            return Err(SqlError::Parse(
                "generation expression must be scalar".into(),
            ));
        }
    };
    Ok(expr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_arithmetic() {
        let expr = parse_generation_expression("n + 1").unwrap();
        assert!(matches!(expr, Expr::BinaryOp { .. }));
    }
}
