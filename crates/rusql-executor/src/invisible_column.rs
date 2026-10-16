//! Invisible columns / indexes (M150).

use crate::ExecError;
use rusql_core::{ColumnDef, TableMeta};
use rusql_sql::INVISIBLE_COMMENT_MARKER;
use rusql_storage::Row;

pub fn invisible_from_comment(comment: &str) -> bool {
    comment.trim_matches(['\'', '"']).trim() == INVISIBLE_COMMENT_MARKER
}

pub fn apply_invisible_from_column_options(
    col: &mut ColumnDef,
    opt: &sqlparser::ast::ColumnOption,
) {
    match opt {
        sqlparser::ast::ColumnOption::Comment(c) if invisible_from_comment(c) => {
            col.invisible = true;
        }
        sqlparser::ast::ColumnOption::DialectSpecific(tokens)
            if tokens
                .iter()
                .any(|t| t.to_string().eq_ignore_ascii_case("INVISIBLE")) =>
        {
            col.invisible = true;
        }
        _ => {}
    }
}

pub fn validate_invisible_insert(
    meta: &TableMeta,
    row: &Row,
    provided: &[bool],
) -> Result<(), ExecError> {
    for (i, col) in meta.columns.iter().enumerate() {
        if col.is_virtual_generated() || !col.invisible {
            continue;
        }
        if col.default.is_some() {
            continue;
        }
        if !provided[i] || row[i].is_empty() {
            return Err(ExecError::Mysql {
                code: 1364,
                message: rusql_i18n::messages::sql_column_has_no_default(&col.name),
            });
        }
    }
    Ok(())
}

pub fn indices_for_wildcard(
    table_columns: &[String],
    wildcard_columns: &[String],
) -> Result<Vec<usize>, ExecError> {
    wildcard_columns
        .iter()
        .map(|name| {
            table_columns
                .iter()
                .position(|c| c.eq_ignore_ascii_case(name))
                .ok_or_else(|| {
                    ExecError::Message(format!("unknown column '{name}' in SELECT * expansion"))
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_planner::plan;
    use rusql_sql::parse;
    use rusql_storage::HeapEngine;

    fn exec_ok(
        exec: &mut crate::Executor<HeapEngine>,
        session: &mut rusql_core::Session,
        sql: &str,
    ) {
        let plans = plan(session, parse(sql).unwrap());
        exec.execute(session, &plans, None).unwrap();
    }

    fn query_rows(
        exec: &mut crate::Executor<HeapEngine>,
        session: &mut rusql_core::Session,
        sql: &str,
    ) -> Vec<Vec<String>> {
        let plans = plan(session, parse(sql).unwrap());
        match &exec.execute(session, &plans, None).unwrap()[0] {
            crate::QueryResult::Rows { rows, .. } => rows.clone(),
            other => panic!("expected rows for {sql}, got {other:?}"),
        }
    }

    #[test]
    fn invisible_column_select_star_omits_column() {
        let mut exec = crate::Executor::new(HeapEngine::new());
        let mut session = rusql_core::Session::new(1, "root");
        exec_ok(
            &mut exec,
            &mut session,
            "CREATE TABLE inv_t (id INT PRIMARY KEY, n INT INVISIBLE)",
        );
        exec_ok(
            &mut exec,
            &mut session,
            "INSERT INTO inv_t (id, n) VALUES (1, 42)",
        );
        let rows = query_rows(&mut exec, &mut session, "SELECT * FROM inv_t");
        assert_eq!(rows, vec![vec!["1".to_string()]]);
        let rows = query_rows(&mut exec, &mut session, "SELECT n FROM inv_t");
        assert_eq!(rows, vec![vec!["42".to_string()]]);
    }

    #[test]
    fn invisible_column_insert_requires_value_without_default() {
        let mut exec = crate::Executor::new(HeapEngine::new());
        let mut session = rusql_core::Session::new(1, "root");
        exec_ok(
            &mut exec,
            &mut session,
            "CREATE TABLE inv_req (id INT PRIMARY KEY, n INT INVISIBLE)",
        );
        let plans = plan(
            &session,
            parse("INSERT INTO inv_req (id) VALUES (1)").unwrap(),
        );
        assert!(matches!(
            exec.execute(&mut session, &plans, None).unwrap_err(),
            ExecError::Mysql { code: 1364, .. }
        ));
    }

    #[test]
    fn invisible_column_default_allows_insert_omit() {
        let mut exec = crate::Executor::new(HeapEngine::new());
        let mut session = rusql_core::Session::new(1, "root");
        exec_ok(
            &mut exec,
            &mut session,
            "CREATE TABLE inv_def (id INT PRIMARY KEY, n INT INVISIBLE DEFAULT 9)",
        );
        exec_ok(
            &mut exec,
            &mut session,
            "INSERT INTO inv_def (id) VALUES (1)",
        );
        let rows = query_rows(&mut exec, &mut session, "SELECT n FROM inv_def");
        assert_eq!(rows[0][0], "9");
    }

    #[test]
    fn invisible_index_not_used_by_explain() {
        let mut exec = crate::Executor::new(HeapEngine::new());
        let mut session = rusql_core::Session::new(1, "root");
        for sql in [
            "CREATE TABLE idx_inv (id INT PRIMARY KEY, n INT)",
            "CREATE INDEX idx_n ON idx_inv (n) INVISIBLE",
            "INSERT INTO idx_inv (id, n) VALUES (1, 7), (2, 9)",
        ] {
            exec_ok(&mut exec, &mut session, sql);
        }
        let rows = query_rows(
            &mut exec,
            &mut session,
            "EXPLAIN SELECT id FROM idx_inv WHERE n = 7",
        );
        assert_eq!(rows[0][3], "ALL");
    }

    #[test]
    fn show_create_table_includes_invisible() {
        let mut exec = crate::Executor::new(HeapEngine::new());
        let mut session = rusql_core::Session::new(1, "root");
        exec_ok(
            &mut exec,
            &mut session,
            "CREATE TABLE inv_show (id INT PRIMARY KEY, n INT INVISIBLE)",
        );
        let rows = query_rows(&mut exec, &mut session, "SHOW CREATE TABLE inv_show");
        assert!(rows[0][1].contains("INVISIBLE"), "ddl: {}", rows[0][1]);
    }
}
