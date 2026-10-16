//! MySQL column type normalization (M40, M148 ENUM/SET).

/// Base type name without length/precision (uppercase, no params).
pub fn type_base(data_type: &str) -> String {
    data_type
        .trim()
        .split('(')
        .next()
        .unwrap_or(data_type)
        .trim()
        .to_uppercase()
}

/// `information_schema.COLUMNS.DATA_TYPE` (lowercase base, no params).
pub fn data_type_name(data_type: &str) -> String {
    type_base(data_type).to_lowercase()
}

/// `DESCRIBE` / `COLUMN_TYPE` display (lowercase, keeps `(p,s)` / length).
pub fn column_type_display(data_type: &str) -> String {
    data_type.trim().to_lowercase()
}

/// `SHOW CREATE TABLE` type fragment: uppercase base name, preserve `(…)` including ENUM/SET members.
pub fn column_type_show_create(data_type: &str) -> String {
    let trimmed = data_type.trim();
    if trimmed.is_empty() {
        return trimmed.to_string();
    }
    let open = trimmed.find('(').unwrap_or(trimmed.len());
    format!(
        "{}{}",
        trimmed[..open].to_ascii_uppercase(),
        &trimmed[open..]
    )
}

/// Normalize type string from CREATE TABLE for catalog storage.
pub fn normalize_column_type(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return trimmed.to_string();
    }
    let base = type_base(trimmed);
    let rest = trimmed
        .split_once('(')
        .map(|(_, params)| format!("({params}"))
        .unwrap_or_default();
    format!("{base}{rest}")
}

/// ENUM vs SET column (M148).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnumSetKind {
    Enum,
    Set,
}

/// Parse member list from `ENUM('a',…)` / `SET('a',…)` catalog type strings.
pub fn enum_set_members(data_type: &str) -> Option<(EnumSetKind, Vec<String>)> {
    let base = type_base(data_type);
    let kind = match base.as_str() {
        "ENUM" => EnumSetKind::Enum,
        "SET" => EnumSetKind::Set,
        _ => return None,
    };
    let trimmed = data_type.trim();
    let open = trimmed.find('(')?;
    let close = trimmed.rfind(')')?;
    if close <= open {
        return Some((kind, Vec::new()));
    }
    Some((kind, parse_mysql_quoted_list(&trimmed[open + 1..close])))
}

fn parse_mysql_quoted_list(inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = inner.chars().peekable();
    while skip_list_whitespace(&mut chars) {
        if chars.peek() != Some(&'\'') {
            break;
        }
        chars.next();
        let mut val = String::new();
        loop {
            match chars.next() {
                None => break,
                Some('\'') => {
                    if chars.peek() == Some(&'\'') {
                        chars.next();
                        val.push('\'');
                    } else {
                        out.push(val);
                        break;
                    }
                }
                Some(c) => val.push(c),
            }
        }
        if chars.peek() == Some(&',') {
            chars.next();
        }
    }
    out
}

fn skip_list_whitespace(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> bool {
    while matches!(chars.peek(), Some(' ' | '\t' | '\n' | '\r')) {
        chars.next();
    }
    chars.peek().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_type_names() {
        assert_eq!(data_type_name("DECIMAL(10,2)"), "decimal");
        assert_eq!(column_type_display("DECIMAL(10,2)"), "decimal(10,2)");
        assert_eq!(normalize_column_type("decimal(10, 2)"), "DECIMAL(10, 2)");
    }

    #[test]
    fn datetime_and_json() {
        assert_eq!(data_type_name("DATETIME"), "datetime");
        assert_eq!(data_type_name("JSON"), "json");
        assert_eq!(column_type_display("TEXT"), "text");
    }

    #[test]
    fn enum_set_member_parse() {
        let (kind, members) = enum_set_members("ENUM('a','b')").unwrap();
        assert_eq!(kind, EnumSetKind::Enum);
        assert_eq!(members, vec!["a", "b"]);
        let (kind, members) = enum_set_members("set('x','y','z')").unwrap();
        assert_eq!(kind, EnumSetKind::Set);
        assert_eq!(members, vec!["x", "y", "z"]);
        let (kind, members) = enum_set_members("ENUM('it''s')").unwrap();
        assert_eq!(kind, EnumSetKind::Enum);
        assert_eq!(members, vec!["it's"]);
        assert!(enum_set_members("INT").is_none());
    }
}
