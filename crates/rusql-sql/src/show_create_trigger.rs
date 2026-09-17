//! Rewrite `SHOW CREATE TRIGGER` for sqlparser
//! (no `ShowCreateObject::Trigger` in 0.53).

/// Internal virtual table the executor recognizes for `SHOW CREATE TRIGGER`.
pub const CREATE_TRIGGER_VIRTUAL_TABLE: &str = "__rusql_create_trigger";

/// Parsed `SHOW CREATE TRIGGER [db.]name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowCreateTrigger {
    pub database: Option<String>,
    pub name: String,
}

/// If `sql` is `SHOW CREATE TRIGGER ident` (optional `db.ident`, trailing `;`).
pub fn parse_show_create_trigger(sql: &str) -> Option<ShowCreateTrigger> {
    let s = sql.trim().trim_end_matches(';').trim();
    let rest = skip_keyword(s, "SHOW")?;
    let rest = skip_keyword(rest, "CREATE")?;
    let rest = skip_keyword(rest, "TRIGGER")?;
    let (first, rest) = take_ident(rest)?;
    if first.is_empty() {
        return None;
    }
    let rest = rest.trim_start();
    if let Some(after_dot) = rest.strip_prefix('.') {
        let (second, rest) = take_ident(after_dot)?;
        if second.is_empty() || !rest.trim().is_empty() {
            return None;
        }
        return Some(ShowCreateTrigger {
            database: Some(first),
            name: second,
        });
    }
    if !rest.is_empty() {
        return None;
    }
    Some(ShowCreateTrigger {
        database: None,
        name: first,
    })
}

/// Rewrite to an internal SELECT the executor recognizes.
pub fn rewrite_show_create_trigger(sql: &str) -> Option<String> {
    let parsed = parse_show_create_trigger(sql)?;
    let mut parts = vec![format!("__name__ = '{}'", escape_sql_string(&parsed.name))];
    if let Some(db) = parsed.database {
        parts.push(format!("__db__ = '{}'", escape_sql_string(&db)));
    }
    Some(format!(
        "SELECT * FROM {CREATE_TRIGGER_VIRTUAL_TABLE} WHERE {}",
        parts.join(" AND ")
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
    fn parse_plain_and_qualified() {
        assert_eq!(
            parse_show_create_trigger("SHOW CREATE TRIGGER tr_src"),
            Some(ShowCreateTrigger {
                database: None,
                name: "tr_src".into(),
            })
        );
        assert_eq!(
            parse_show_create_trigger("show create trigger `tr_src`;"),
            Some(ShowCreateTrigger {
                database: None,
                name: "tr_src".into(),
            })
        );
        assert_eq!(
            parse_show_create_trigger("SHOW CREATE TRIGGER rusql.`tr_src`"),
            Some(ShowCreateTrigger {
                database: Some("rusql".into()),
                name: "tr_src".into(),
            })
        );
    }

    #[test]
    fn ignores_neighbors() {
        assert!(parse_show_create_trigger("SHOW CREATE TABLE users").is_none());
        assert!(parse_show_create_trigger("SHOW CREATE VIEW v").is_none());
        assert!(parse_show_create_trigger("SHOW CREATE DATABASE rusql").is_none());
        assert!(parse_show_create_trigger("SHOW CREATE PROCEDURE p").is_none());
        assert!(parse_show_create_trigger("SHOW CREATE FUNCTION f").is_none());
        assert!(parse_show_create_trigger("SHOW TRIGGERS").is_none());
        assert!(parse_show_create_trigger("SHOW CREATE TRIGGER").is_none());
        assert!(parse_show_create_trigger("SELECT 1").is_none());
    }

    #[test]
    fn rewrite_to_internal_select() {
        assert_eq!(
            rewrite_show_create_trigger("SHOW CREATE TRIGGER tr_src").as_deref(),
            Some("SELECT * FROM __rusql_create_trigger WHERE __name__ = 'tr_src'")
        );
        let sql = rewrite_show_create_trigger("SHOW CREATE TRIGGER rusql.`tr_src`").unwrap();
        assert!(sql.contains("__rusql_create_trigger"));
        assert!(sql.contains("__name__ = 'tr_src'"));
        assert!(sql.contains("__db__ = 'rusql'"));
    }
}
