//! Composite BTree key encoding for multi-column secondary indexes.

const SEP: &str = "\u{001f}";

/// Encode ordered index column values into a single BTree key.
pub fn encode_index_key(values: &[String]) -> String {
    values.join(SEP)
}

/// Inclusive range bounds for equality on an index prefix (e.g. `(a)` on index `(a, b)`).
pub fn prefix_range_bounds(first_value: &str) -> (String, String) {
    let low = format!("{first_value}{SEP}");
    let high = prefix_upper_bound(first_value);
    (low, high)
}

/// Inclusive range bounds for `BETWEEN` on the leading indexed column.
pub fn leading_between_bounds(low_value: &str, high_value: &str) -> (String, String) {
    (
        format!("{low_value}{SEP}"),
        format!("{high_value}{SEP}\u{ffff}"),
    )
}

fn prefix_upper_bound(prefix: &str) -> String {
    let mut chars: Vec<char> = prefix.chars().collect();
    while let Some(ch) = chars.pop() {
        if ch != char::MAX {
            chars.push(char::from_u32(ch as u32 + 1).unwrap_or(ch));
            return chars.into_iter().collect();
        }
    }
    "\u{ffff}".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_and_prefix_range() {
        assert_eq!(encode_index_key(&["1".into(), "2".into()]), "1\u{001f}2");
        let (low, high) = prefix_range_bounds("1");
        assert!(low.as_str() < "1\u{001f}2");
        assert!("1\u{001f}2" <= high.as_str());
    }
}
