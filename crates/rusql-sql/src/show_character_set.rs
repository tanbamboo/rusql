//! Rewrite `SHOW CHARACTER SET` / `SHOW CHARSET` for sqlparser
//! (no `Statement::ShowCharset` in 0.53).

/// Internal virtual table the executor recognizes for `SHOW CHARACTER SET`.
pub const CHARACTER_SET_VIRTUAL_TABLE: &str = "__rusql_character_sets";

/// Parsed `SHOW CHARACTER SET [LIKE 'pattern']` / `SHOW CHARSET [LIKE 'pattern']`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowCharacterSet {
    pub like: Option<String>,
}

/// If `sql` is `SHOW CHARACTER SET` / `SHOW CHARSET` (optional `LIKE`), return the parsed form.
pub fn parse_show_character_set(sql: &str) -> Option<ShowCharacterSet> {
    let s = sql.trim().trim_end_matches(';').trim();
    let rest = skip_keyword(s, "SHOW")?;
    let rest = if let Some(after) = skip_keyword(rest, "CHARACTER") {
        skip_keyword(after, "SET")?
    } else {
        skip_keyword(rest, "CHARSET")?
    };

    let mut rest = rest.trim_start();
    let mut like = None;
    if let Some(after) = skip_keyword(rest, "LIKE") {
        let (pattern, after) = take_string(after)?;
        like = Some(pattern);
        rest = after.trim_start();
    }

    if !rest.is_empty() {
        return None;
    }
    Some(ShowCharacterSet { like })
}

/// Rewrite to an internal SELECT the executor recognizes.
pub fn rewrite_show_character_set(sql: &str) -> Option<String> {
    let parsed = parse_show_character_set(sql)?;
    if let Some(pattern) = parsed.like {
        Some(format!(
            "SELECT * FROM {CHARACTER_SET_VIRTUAL_TABLE} WHERE __like__ = '{}'",
            escape_sql_string(&pattern)
        ))
    } else {
        Some(format!("SELECT * FROM {CHARACTER_SET_VIRTUAL_TABLE}"))
    }
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

fn take_string(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    let rest = s.strip_prefix('\'')?;
    let mut out = String::new();
    let mut chars = rest.char_indices();
    while let Some((i, ch)) = chars.next() {
        if ch == '\'' {
            if rest[i + ch.len_utf8()..].starts_with('\'') {
                out.push('\'');
                chars.next();
                continue;
            }
            return Some((out, &rest[i + ch.len_utf8()..]));
        }
        out.push(ch);
    }
    None
}

fn escape_sql_string(s: &str) -> String {
    s.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plain_and_like() {
        assert_eq!(
            parse_show_character_set("SHOW CHARACTER SET"),
            Some(ShowCharacterSet { like: None })
        );
        assert_eq!(
            parse_show_character_set("show charset;"),
            Some(ShowCharacterSet { like: None })
        );
        assert_eq!(
            parse_show_character_set("SHOW CHARSET LIKE 'utf8%'"),
            Some(ShowCharacterSet {
                like: Some("utf8%".into()),
            })
        );
        assert_eq!(
            parse_show_character_set("SHOW CHARACTER SET LIKE 'it''s'"),
            Some(ShowCharacterSet {
                like: Some("it's".into()),
            })
        );
    }

    #[test]
    fn ignores_set_charset_and_neighbors() {
        assert!(parse_show_character_set("SET CHARACTER SET utf8mb4").is_none());
        assert!(parse_show_character_set("SET CHARSET utf8mb4").is_none());
        assert!(parse_show_character_set("SHOW COLLATION").is_none());
        assert!(parse_show_character_set("SHOW ENGINES").is_none());
        assert!(parse_show_character_set("SHOW CHARACTER SET WHERE Charset = 'utf8mb4'").is_none());
        assert!(parse_show_character_set("SELECT 1").is_none());
    }

    #[test]
    fn rewrite_to_internal_select() {
        assert_eq!(
            rewrite_show_character_set("SHOW CHARACTER SET").as_deref(),
            Some("SELECT * FROM __rusql_character_sets")
        );
        assert_eq!(
            rewrite_show_character_set("SHOW CHARSET").as_deref(),
            Some("SELECT * FROM __rusql_character_sets")
        );
        let sql = rewrite_show_character_set("SHOW CHARACTER SET LIKE 'utf8%'").unwrap();
        assert!(sql.contains("__rusql_character_sets"));
        assert!(sql.contains("__like__ = 'utf8%'"));
    }
}
