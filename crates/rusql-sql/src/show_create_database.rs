//! Rewrite `SHOW CREATE DATABASE` / `SHOW CREATE SCHEMA` for sqlparser
//! (no `ShowCreateObject::Database` in 0.53).

/// Internal virtual table the executor recognizes for `SHOW CREATE DATABASE`.
pub const CREATE_DATABASE_VIRTUAL_TABLE: &str = "__rusql_create_database";

/// Parsed `SHOW CREATE DATABASE db` / `SHOW CREATE SCHEMA db`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowCreateDatabase {
    pub database: String,
}

/// If `sql` is `SHOW CREATE DATABASE|SCHEMA ident` (optional trailing `;`).
pub fn parse_show_create_database(sql: &str) -> Option<ShowCreateDatabase> {
    let s = sql.trim().trim_end_matches(';').trim();
    let rest = skip_keyword(s, "SHOW")?;
    let rest = skip_keyword(rest, "CREATE")?;
    let rest = skip_keyword(rest, "DATABASE").or_else(|| skip_keyword(rest, "SCHEMA"))?;
    let (name, rest) = take_ident(rest)?;
    if name.is_empty() || !rest.trim().is_empty() {
        return None;
    }
    Some(ShowCreateDatabase { database: name })
}

/// Rewrite to an internal SELECT the executor recognizes.
pub fn rewrite_show_create_database(sql: &str) -> Option<String> {
    let parsed = parse_show_create_database(sql)?;
    Some(format!(
        "SELECT * FROM {CREATE_DATABASE_VIRTUAL_TABLE} WHERE __db__ = '{}'",
        escape_sql_string(&parsed.database)
    ))
}

fn skip_keyword<'a>(s: &'a str, keyword: &str) -> Option<&'a str> {
    let s = s.trim_start();
    if s.len() < keyword.len() {
        return None;
    }
    if !s.get(..keyword.len())?.eq_ignore_ascii_case(keyword) {
        return None;
    }
    let after = &s[keyword.len()..];
    if after.is_empty() || after.starts_with(|c: char| c.is_ascii_whitespace()) {
        Some(after)
    } else {
        None
    }
}

fn take_ident(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    if let Some(rest) = s.strip_prefix('`') {
        let end = rest.find('`')?;
        let name = rest[..end].to_string();
        Some((name, &rest[end + 1..]))
    } else {
        let n = s
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
            .unwrap_or(s.len());
        if n == 0 {
            return None;
        }
        Some((s[..n].to_string(), &s[n..]))
    }
}

fn escape_sql_string(s: &str) -> String {
    s.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_database_and_schema() {
        assert_eq!(
            parse_show_create_database("SHOW CREATE DATABASE rusql"),
            Some(ShowCreateDatabase {
                database: "rusql".into(),
            })
        );
        assert_eq!(
            parse_show_create_database("show create schema `app_db`;"),
            Some(ShowCreateDatabase {
                database: "app_db".into(),
            })
        );
        assert_eq!(
            parse_show_create_database("SHOW CREATE DATABASE `rusql`"),
            Some(ShowCreateDatabase {
                database: "rusql".into(),
            })
        );
    }

    #[test]
    fn ignores_show_create_table_and_neighbors() {
        assert!(parse_show_create_database("SHOW CREATE TABLE users").is_none());
        assert!(parse_show_create_database("SHOW CREATE VIEW v").is_none());
        assert!(parse_show_create_database("SHOW CREATE PROCEDURE p").is_none());
        assert!(parse_show_create_database("SHOW CREATE FUNCTION f").is_none());
        assert!(parse_show_create_database("SHOW CREATE TRIGGER t").is_none());
        assert!(parse_show_create_database("SHOW CREATE DATABASE").is_none());
        assert!(parse_show_create_database("SHOW CREATE DATABASE IF NOT EXISTS rusql").is_none());
        assert!(parse_show_create_database("SHOW WARNINGS").is_none());
        assert!(parse_show_create_database("SHOW DATABASES").is_none());
        assert!(parse_show_create_database("SELECT 1").is_none());
    }

    #[test]
    fn rewrite_to_internal_select() {
        assert_eq!(
            rewrite_show_create_database("SHOW CREATE DATABASE rusql").as_deref(),
            Some("SELECT * FROM __rusql_create_database WHERE __db__ = 'rusql'")
        );
        let sql = rewrite_show_create_database("SHOW CREATE SCHEMA `app_db`").unwrap();
        assert!(sql.contains("__rusql_create_database"));
        assert!(sql.contains("__db__ = 'app_db'"));
    }
}
