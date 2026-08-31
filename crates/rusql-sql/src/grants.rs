//! Normalize MySQL GRANT/REVOKE object syntax for sqlparser.

/// Rewrite `ON db.*` to `ON ALL TABLES IN SCHEMA db` for sqlparser.
pub fn rewrite_grant_objects(sql: &str) -> String {
    let upper = sql.to_ascii_uppercase();
    let on_idx = match upper.find(" ON ") {
        Some(i) => i,
        None => return sql.to_string(),
    };
    let after_on = &sql[on_idx + 4..];
    let (object, rest_start) = match parse_grant_object(after_on) {
        Some(v) => v,
        None => return sql.to_string(),
    };
    let prefix = &sql[..on_idx + 4];
    let rest = &after_on[rest_start..];
    if object.ends_with(".*") {
        let schema = object.trim_end_matches(".*");
        format!("{prefix}ALL TABLES IN SCHEMA {schema}{rest}")
    } else {
        sql.to_string()
    }
}

fn parse_grant_object(input: &str) -> Option<(String, usize)> {
    let trimmed = input.trim_start();
    let offset = input.len() - trimmed.len();
    if let Some(stripped) = trimmed.strip_prefix('`') {
        let end = stripped.find('`')? + 1;
        let mut object = stripped[..end - 1].to_string();
        let mut pos = end + 1;
        if trimmed.get(pos..)?.starts_with(".*") {
            object.push_str(".*");
            pos += 2;
        }
        return Some((object, offset + pos));
    }
    let end = trimmed
        .find(|c: char| c.is_whitespace())
        .unwrap_or(trimmed.len());
    let object = trimmed[..end].to_string();
    Some((object, offset + end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_schema_wildcard() {
        let sql = rewrite_grant_objects("GRANT SELECT ON rusql.* TO app");
        assert!(sql.contains("ALL TABLES IN SCHEMA rusql"));
    }
}
