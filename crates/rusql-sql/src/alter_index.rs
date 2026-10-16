//! MySQL dump-style `ALTER TABLE … ADD/DROP INDEX` and `DROP INDEX … ON` rewrites (M153).
//!
//! sqlparser 0.53 parses `ALTER TABLE t DROP INDEX idx` as `DROP COLUMN idx`; rewrite to
//! `DROP CONSTRAINT` which maps to index drop in the executor.

fn trim_stmt(sql: &str) -> &str {
    sql.trim().trim_end_matches(';').trim()
}

fn unquote_ident(token: &str) -> String {
    let t = token.trim();
    if t.len() >= 2 && t.starts_with('`') && t.ends_with('`') {
        t[1..t.len() - 1].replace("``", "`")
    } else {
        t.to_string()
    }
}

fn parse_drop_index_on_table(sql: &str) -> Option<(String, String)> {
    let s = trim_stmt(sql);
    let upper = s.to_ascii_uppercase();
    if !upper.starts_with("DROP INDEX ") && !upper.starts_with("DROP KEY ") {
        return None;
    }
    let rest = if upper.starts_with("DROP INDEX ") {
        s[11..].trim_start()
    } else {
        s[9..].trim_start()
    };
    let on_pos = rest.to_ascii_uppercase().find(" ON ")?;
    let index_part = rest[..on_pos].trim();
    let table_part = rest[on_pos + 4..].trim();
    if index_part.is_empty() || table_part.is_empty() {
        return None;
    }
    let index_name = index_part.split_whitespace().next()?;
    let table_name = table_part.split_whitespace().next()?;
    Some((unquote_ident(table_name), unquote_ident(index_name)))
}

fn parse_alter_table_drop_index(sql: &str) -> Option<(String, String)> {
    let s = trim_stmt(sql);
    let upper = s.to_ascii_uppercase();
    if !upper.starts_with("ALTER TABLE ") {
        return None;
    }
    let rest = s[12..].trim_start();
    let drop_pos = rest.to_ascii_uppercase().find(" DROP ")?;
    let table_part = rest[..drop_pos].trim();
    let after_drop = rest[drop_pos + 6..].trim_start();
    let upper_after = after_drop.to_ascii_uppercase();
    let index_name = if upper_after.starts_with("INDEX ") {
        after_drop[6..].trim_start()
    } else if upper_after.starts_with("KEY ") {
        after_drop[4..].trim_start()
    } else {
        return None;
    };
    let name_token = index_name.split_whitespace().next()?;
    if table_part.is_empty() || name_token.is_empty() {
        return None;
    }
    Some((table_part.to_string(), unquote_ident(name_token)))
}

/// Rewrite dump-style index DDL so sqlparser + executor accept it.
pub fn rewrite_alter_index_ddl(sql: &str) -> String {
    if let Some((table, index)) = parse_drop_index_on_table(sql) {
        return format!("ALTER TABLE {table} DROP CONSTRAINT {index}");
    }
    if let Some((table, index)) = parse_alter_table_drop_index(sql) {
        return format!("ALTER TABLE {table} DROP CONSTRAINT {index}");
    }
    sql.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use sqlparser::ast::{AlterTableOperation, Statement, TableConstraint};

    #[test]
    fn alter_index_parse_add_index() {
        let stmts = parse("ALTER TABLE t ADD INDEX idx_n (n)").unwrap();
        match &stmts[0] {
            Statement::AlterTable { operations, .. } => match &operations[0] {
                AlterTableOperation::AddConstraint(TableConstraint::Index { name, .. }) => {
                    assert_eq!(name.as_ref().map(|i| i.value.as_str()), Some("idx_n"));
                }
                other => panic!("expected ADD INDEX constraint, got {other:?}"),
            },
            other => panic!("expected ALTER TABLE, got {other:?}"),
        }
    }

    #[test]
    fn alter_index_parse_add_unique() {
        let stmts = parse("ALTER TABLE t ADD UNIQUE (n)").unwrap();
        match &stmts[0] {
            Statement::AlterTable { operations, .. } => match &operations[0] {
                AlterTableOperation::AddConstraint(TableConstraint::Unique { .. }) => {}
                other => panic!("expected ADD UNIQUE, got {other:?}"),
            },
            other => panic!("expected ALTER TABLE, got {other:?}"),
        }
    }

    #[test]
    fn alter_index_rewrite_drop_index_on_table() {
        let rewritten = rewrite_alter_index_ddl("DROP INDEX idx_n ON t");
        assert_eq!(rewritten, "ALTER TABLE t DROP CONSTRAINT idx_n");
        let stmts = parse("DROP INDEX idx_n ON t").unwrap();
        match &stmts[0] {
            Statement::AlterTable { operations, .. } => match &operations[0] {
                AlterTableOperation::DropConstraint { name, .. } => {
                    assert_eq!(name.value, "idx_n");
                }
                other => panic!("expected DROP CONSTRAINT, got {other:?}"),
            },
            other => panic!("expected ALTER TABLE, got {other:?}"),
        }
    }

    #[test]
    fn alter_index_rewrite_alter_drop_index() {
        let rewritten = rewrite_alter_index_ddl("ALTER TABLE t DROP INDEX idx_n");
        assert_eq!(rewritten, "ALTER TABLE t DROP CONSTRAINT idx_n");
        let stmts = parse("ALTER TABLE t DROP INDEX idx_n").unwrap();
        match &stmts[0] {
            Statement::AlterTable { operations, .. } => match &operations[0] {
                AlterTableOperation::DropConstraint { name, .. } => {
                    assert_eq!(name.value, "idx_n");
                }
                other => panic!("expected DROP CONSTRAINT, got {other:?}"),
            },
            other => panic!("expected ALTER TABLE, got {other:?}"),
        }
    }
}
