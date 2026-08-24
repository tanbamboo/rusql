//! MySQL column type normalization (M40).

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
}
