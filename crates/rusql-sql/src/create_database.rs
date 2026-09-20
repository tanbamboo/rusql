//! Parse/rewrite `CREATE DATABASE … CHARACTER SET … COLLATE …` for sqlparser 0.53.
//!
//! sqlparser `Statement::CreateDatabase` has no charset fields. Options are encoded
//! in a documented internal `LOCATION` payload the executor decodes:
//! `__rusql_db__charset=<cs>;collation=<col>`.

/// Prefix of the documented internal LOCATION encoding (M114).
pub const CREATE_DATABASE_META_PREFIX: &str = "__rusql_db__";

/// Parsed `CREATE DATABASE` / `CREATE SCHEMA` including optional charset clauses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateDatabaseOptions {
    pub name: String,
    pub if_not_exists: bool,
    pub character_set: Option<String>,
    pub collation: Option<String>,
}

/// Parse `CREATE DATABASE|SCHEMA [IF NOT EXISTS] name [charset/collate options]`.
pub fn parse_create_database_options(sql: &str) -> Option<CreateDatabaseOptions> {
    let s = sql.trim().trim_end_matches(';').trim();
    let rest = skip_keyword(s, "CREATE")?;
    let rest = skip_keyword(rest, "DATABASE").or_else(|| skip_keyword(rest, "SCHEMA"))?;
    let (if_not_exists, rest) = take_if_not_exists(rest);
    let (name, rest) = take_ident(rest)?;
    if name.is_empty() {
        return None;
    }
    let rest = rest.trim_start();
    if rest.is_empty() {
        return Some(CreateDatabaseOptions {
            name,
            if_not_exists,
            character_set: None,
            collation: None,
        });
    }
    let (character_set, collation, leftover) = parse_charset_collate_options(rest)?;
    if !leftover.trim().is_empty() {
        return None;
    }
    Some(CreateDatabaseOptions {
        name,
        if_not_exists,
        character_set,
        collation,
    })
}

/// Rewrite charset/collate clauses to `CREATE DATABASE [IF NOT EXISTS] name LOCATION '…'`.
///
/// Returns `None` when there are no charset/collate clauses (plain `CREATE DATABASE name`).
pub fn rewrite_create_database_charset(sql: &str) -> Option<String> {
    let parsed = parse_create_database_options(sql)?;
    if parsed.character_set.is_none() && parsed.collation.is_none() {
        return None;
    }
    let location = encode_create_database_location(
        parsed.character_set.as_deref(),
        parsed.collation.as_deref(),
    );
    let ident = parsed.name.replace('`', "``");
    let ine = if parsed.if_not_exists {
        "IF NOT EXISTS "
    } else {
        ""
    };
    Some(format!(
        "CREATE DATABASE {ine}`{ident}` LOCATION '{location}'"
    ))
}

/// Encode charset/collation into the documented LOCATION payload.
pub fn encode_create_database_location(
    character_set: Option<&str>,
    collation: Option<&str>,
) -> String {
    let mut parts = Vec::new();
    if let Some(cs) = character_set {
        parts.push(format!("charset={cs}"));
    }
    if let Some(col) = collation {
        parts.push(format!("collation={col}"));
    }
    format!("{CREATE_DATABASE_META_PREFIX}{}", parts.join(";"))
}

/// Decode a documented LOCATION payload into charset/collation options.
pub fn decode_create_database_location(location: Option<&str>) -> (Option<String>, Option<String>) {
    let Some(loc) = location else {
        return (None, None);
    };
    let Some(rest) = loc.strip_prefix(CREATE_DATABASE_META_PREFIX) else {
        return (None, None);
    };
    let mut character_set = None;
    let mut collation = None;
    for part in rest.split(';') {
        if let Some(v) = part.strip_prefix("charset=") {
            if !v.is_empty() {
                character_set = Some(v.to_string());
            }
        } else if let Some(v) = part.strip_prefix("collation=") {
            if !v.is_empty() {
                collation = Some(v.to_string());
            }
        }
    }
    (character_set, collation)
}

fn parse_charset_collate_options(mut rest: &str) -> Option<(Option<String>, Option<String>, &str)> {
    let mut character_set = None;
    let mut collation = None;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let Some((kind, value, next)) = take_charset_or_collate(rest) else {
            break;
        };
        match kind {
            OptionKind::Charset => character_set = Some(value),
            OptionKind::Collation => collation = Some(value),
        }
        rest = next;
    }
    Some((character_set, collation, rest))
}

enum OptionKind {
    Charset,
    Collation,
}

fn take_charset_or_collate(s: &str) -> Option<(OptionKind, String, &str)> {
    let s = strip_optional_default(s);
    if let Some(after) = skip_keyword(s, "CHARACTER") {
        let after_set = skip_keyword(after.trim_start(), "SET")?;
        let after_eq = skip_optional_eq(after_set);
        let (val, rest) = take_value(after_eq)?;
        return Some((OptionKind::Charset, val, rest));
    }
    if let Some(after) = skip_keyword(s, "CHARSET") {
        let after_eq = skip_optional_eq(after);
        let (val, rest) = take_value(after_eq)?;
        return Some((OptionKind::Charset, val, rest));
    }
    if let Some(after) = skip_keyword(s, "COLLATE") {
        let after_eq = skip_optional_eq(after);
        let (val, rest) = take_value(after_eq)?;
        return Some((OptionKind::Collation, val, rest));
    }
    None
}

fn strip_optional_default(s: &str) -> &str {
    let s = s.trim_start();
    if let Some(after) = skip_keyword(s, "DEFAULT") {
        let t = after.trim_start();
        if starts_charset_or_collate(t) {
            return after;
        }
    }
    s
}

fn starts_charset_or_collate(s: &str) -> bool {
    skip_keyword(s, "CHARACTER").is_some()
        || skip_keyword(s, "CHARSET").is_some()
        || skip_keyword(s, "COLLATE").is_some()
}

fn take_if_not_exists(s: &str) -> (bool, &str) {
    let s = s.trim_start();
    if let Some(after_if) = skip_keyword(s, "IF") {
        if let Some(after_not) = skip_keyword(after_if.trim_start(), "NOT") {
            if let Some(after_exists) = skip_keyword(after_not.trim_start(), "EXISTS") {
                return (true, after_exists);
            }
        }
    }
    (false, s)
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
    if after.is_empty() || after.starts_with(|c: char| c.is_ascii_whitespace() || c == '=') {
        Some(after)
    } else {
        None
    }
}

fn skip_optional_eq(s: &str) -> &str {
    let s = s.trim_start();
    s.strip_prefix('=').unwrap_or(s)
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

fn take_value(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    if let Some(quote) = s.chars().next().filter(|c| *c == '\'' || *c == '"') {
        let rest = &s[1..];
        let end = rest.find(quote)?;
        return Some((rest[..end].to_string(), &rest[end + 1..]));
    }
    take_ident(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_create_database_charset_collate_and_synonyms() {
        let parsed = parse_create_database_options(
            "CREATE DATABASE gap_cs CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci",
        )
        .unwrap();
        assert_eq!(parsed.name, "gap_cs");
        assert!(!parsed.if_not_exists);
        assert_eq!(parsed.character_set.as_deref(), Some("utf8mb4"));
        assert_eq!(parsed.collation.as_deref(), Some("utf8mb4_unicode_ci"));

        let charset =
            parse_create_database_options("create database gap_cs charset utf8mb4").unwrap();
        assert_eq!(charset.character_set.as_deref(), Some("utf8mb4"));
        assert_eq!(charset.collation, None);

        let defaults = parse_create_database_options(
            "CREATE DATABASE `gap_cs` DEFAULT CHARACTER SET = utf8mb4 DEFAULT COLLATE = utf8mb4_0900_ai_ci",
        )
        .unwrap();
        assert_eq!(defaults.character_set.as_deref(), Some("utf8mb4"));
        assert_eq!(defaults.collation.as_deref(), Some("utf8mb4_0900_ai_ci"));

        let ine = parse_create_database_options(
            "CREATE SCHEMA IF NOT EXISTS gap_cs CHARACTER SET 'utf8mb4'",
        )
        .unwrap();
        assert!(ine.if_not_exists);
        assert_eq!(ine.name, "gap_cs");
        assert_eq!(ine.character_set.as_deref(), Some("utf8mb4"));
    }

    #[test]
    fn parse_create_database_without_clauses() {
        let parsed = parse_create_database_options("CREATE DATABASE app_db").unwrap();
        assert_eq!(parsed.name, "app_db");
        assert_eq!(parsed.character_set, None);
        assert_eq!(parsed.collation, None);
        assert!(rewrite_create_database_charset("CREATE DATABASE app_db").is_none());
    }

    #[test]
    fn rewrite_create_database_encodes_location() {
        let sql = rewrite_create_database_charset(
            "CREATE DATABASE gap_cs CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci",
        )
        .unwrap();
        assert!(sql.starts_with("CREATE DATABASE `gap_cs` LOCATION '"));
        assert!(sql.contains(CREATE_DATABASE_META_PREFIX));
        assert!(sql.contains("charset=utf8mb4"));
        assert!(sql.contains("collation=utf8mb4_unicode_ci"));

        let (cs, col) = decode_create_database_location(Some(
            "__rusql_db__charset=utf8mb4;collation=utf8mb4_unicode_ci",
        ));
        assert_eq!(cs.as_deref(), Some("utf8mb4"));
        assert_eq!(col.as_deref(), Some("utf8mb4_unicode_ci"));
        assert_eq!(decode_create_database_location(None), (None, None));
        assert_eq!(
            decode_create_database_location(Some("s3://bucket")),
            (None, None)
        );
    }

    #[test]
    fn ignores_non_create_database() {
        assert!(parse_create_database_options("CREATE TABLE t (id INT)").is_none());
        assert!(parse_create_database_options("CREATE DATABASE").is_none());
        assert!(rewrite_create_database_charset("SELECT 1").is_none());
    }
}
