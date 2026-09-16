//! Rewrite `SET CHARACTER SET` / `SET CHARSET` to `SET NAMES` (sqlparser 0.53 has no alias).

/// If `sql` is `SET CHARACTER SET …` or `SET CHARSET …`, rewrite to `SET NAMES …`.
///
/// Leaves `SET NAMES`, `SET character_set_client = …`, and non-SET statements untouched.
pub fn rewrite_set_charset(sql: &str) -> Option<String> {
    let start = sql.find(|c: char| !c.is_whitespace())?;
    let rest = &sql[start..];
    if rest.len() < 3 || !rest[..3].eq_ignore_ascii_case("set") {
        return None;
    }
    let after_set = &rest[3..];
    let first = after_set.chars().next()?;
    if !first.is_whitespace() {
        return None;
    }
    let after_ws = after_set.trim_start();
    if let Some(rest) = strip_keyword(after_ws, "character") {
        let after_character = rest.trim_start();
        let after_set_kw = strip_keyword(after_character, "set")?;
        if let Some(c) = after_set_kw.chars().next() {
            if c.is_ascii_alphanumeric() || c == '_' {
                return None;
            }
        }
        let charset_part = after_set_kw.trim_start();
        if charset_part.is_empty() {
            return None;
        }
        return Some(format!("SET NAMES {charset_part}"));
    }
    if let Some(after_charset) = strip_keyword(after_ws, "charset") {
        if let Some(c) = after_charset.chars().next() {
            if c.is_ascii_alphanumeric() || c == '_' {
                return None;
            }
        }
        let charset_part = after_charset.trim_start();
        if charset_part.is_empty() {
            return None;
        }
        return Some(format!("SET NAMES {charset_part}"));
    }
    None
}

fn strip_keyword<'a>(sql: &'a str, keyword: &str) -> Option<&'a str> {
    if sql.len() < keyword.len() || !sql[..keyword.len()].eq_ignore_ascii_case(keyword) {
        return None;
    }
    Some(&sql[keyword.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_character_set_and_charset() {
        assert_eq!(
            rewrite_set_charset("SET CHARACTER SET utf8mb4").as_deref(),
            Some("SET NAMES utf8mb4")
        );
        assert_eq!(
            rewrite_set_charset("set charset utf8mb4").as_deref(),
            Some("SET NAMES utf8mb4")
        );
        assert_eq!(
            rewrite_set_charset("SET CHARACTER SET DEFAULT").as_deref(),
            Some("SET NAMES DEFAULT")
        );
        assert_eq!(
            rewrite_set_charset("SET CHARSET DEFAULT").as_deref(),
            Some("SET NAMES DEFAULT")
        );
    }

    #[test]
    fn rewrite_skips_names_and_sysvars() {
        assert!(rewrite_set_charset("SET NAMES utf8mb4").is_none());
        assert!(rewrite_set_charset("SET character_set_client = utf8mb4").is_none());
        assert!(rewrite_set_charset("SET @@autocommit = 0").is_none());
        assert!(rewrite_set_charset("SELECT 1").is_none());
    }
}
