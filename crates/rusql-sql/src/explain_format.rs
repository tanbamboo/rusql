//! MySQL `EXPLAIN FORMAT=JSON` uses `=`; sqlparser expects `EXPLAIN FORMAT JSON` (M145).

/// Rewrite `EXPLAIN FORMAT=<name>` to `EXPLAIN FORMAT <name>` when the statement is EXPLAIN.
pub fn rewrite_explain_format(sql: &str) -> Option<String> {
    let trimmed_start = sql.trim_start();
    let leading = &sql[..sql.len() - trimmed_start.len()];
    let upper = trimmed_start.to_ascii_uppercase();
    for marker in [
        "EXPLAIN FORMAT=JSON",
        "EXPLAIN FORMAT=TEXT",
        "EXPLAIN FORMAT=GRAPHVIZ",
        "EXPLAIN ANALYZE FORMAT=JSON",
        "EXPLAIN ANALYZE FORMAT=TEXT",
        "EXPLAIN ANALYZE FORMAT=GRAPHVIZ",
        "EXPLAIN VERBOSE FORMAT=JSON",
        "EXPLAIN VERBOSE FORMAT=TEXT",
        "EXPLAIN VERBOSE FORMAT=GRAPHVIZ",
    ] {
        if upper.starts_with(marker) {
            let rest = &trimmed_start[marker.len()..];
            let rewritten = marker.replace("FORMAT=", "FORMAT ");
            return Some(format!("{leading}{rewritten}{rest}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::parse;
    use sqlparser::ast::{AnalyzeFormat, Statement};

    #[test]
    fn explain_format_json_parses() {
        let stmts = parse("EXPLAIN FORMAT=JSON SELECT 1").unwrap();
        match &stmts[0] {
            Statement::Explain {
                format, statement, ..
            } => {
                assert_eq!(*format, Some(AnalyzeFormat::JSON));
                assert!(matches!(statement.as_ref(), Statement::Query(_)));
            }
            other => panic!("expected Explain, got {other:?}"),
        }
    }

    #[test]
    fn plain_explain_unchanged() {
        let stmts = parse("EXPLAIN SELECT 1").unwrap();
        assert!(matches!(stmts[0], Statement::Explain { format: None, .. }));
    }
}
