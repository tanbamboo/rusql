//! Query planner for rusql.

mod access;

pub use access::{
    explain_query_statement, explain_simple_select, AccessType, ExplainPlanRow, IndexInfo,
};

use rusql_core::Session;
use sqlparser::ast::Statement;

/// Logical plan node (MVP: wraps raw statements).
#[derive(Debug, Clone)]
pub enum Plan {
    Statement(Statement),
}

/// Build a plan from parsed statements (MVP: no optimization).
pub fn plan(_session: &Session, statements: Vec<Statement>) -> Vec<Plan> {
    statements.into_iter().map(Plan::Statement).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_sql::parse;

    #[test]
    fn plan_passthrough() {
        let session = Session::new(1, "root");
        let stmts = parse("SELECT 1").unwrap();
        let plans = plan(&session, stmts);
        assert_eq!(plans.len(), 1);
    }
}
