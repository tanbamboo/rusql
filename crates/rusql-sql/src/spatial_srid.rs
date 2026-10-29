//! Rewrite `POINT SRID 4326` so sqlparser accepts it as `POINT(4326)`.
//!
//! The catalog fold in `rusql-core` turns that modifier back into `POINT SRID 4326`.

const SPATIAL_NAMES: &[&str] = &[
    "GEOMETRYCOLLECTION",
    "MULTILINESTRING",
    "MULTIPOLYGON",
    "GEOMCOLLECTION",
    "MULTIPOINT",
    "LINESTRING",
    "POLYGON",
    "GEOMETRY",
    "POINT",
];

/// Replace spatial `TYPE SRID n` clauses outside quotes and comments.
pub fn rewrite_spatial_column_srid(sql: &str) -> String {
    let chars: Vec<char> = sql.chars().collect();
    let mut out = String::with_capacity(sql.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\'' || chars[i] == '"' || chars[i] == '`' {
            i = skip_quoted(&chars, i, &mut out);
            continue;
        }
        if chars[i] == '-' && chars.get(i + 1) == Some(&'-') {
            while i < chars.len() && chars[i] != '\n' {
                out.push(chars[i]);
                i += 1;
            }
            continue;
        }
        if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
            out.push('/');
            out.push('*');
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                out.push(chars[i]);
                i += 1;
            }
            if i + 1 < chars.len() {
                out.push('*');
                out.push('/');
                i += 2;
            }
            continue;
        }
        if let Some((rewritten, next)) = match_srid(&chars, i) {
            out.push_str(&rewritten);
            i = next;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn skip_quoted(chars: &[char], start: usize, out: &mut String) -> usize {
    let quote = chars[start];
    out.push(quote);
    let mut i = start + 1;
    while i < chars.len() {
        out.push(chars[i]);
        if chars[i] == quote {
            if quote == '\'' && chars.get(i + 1) == Some(&'\'') {
                out.push('\'');
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    i
}

fn match_srid(chars: &[char], i: usize) -> Option<(String, usize)> {
    if i > 0 && is_ident(chars[i - 1]) {
        return None;
    }
    for name in SPATIAL_NAMES {
        let name_chars: Vec<char> = name.chars().collect();
        if i + name_chars.len() > chars.len() {
            continue;
        }
        if !chars[i..i + name_chars.len()]
            .iter()
            .zip(name_chars.iter())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
        {
            continue;
        }
        let mut j = i + name_chars.len();
        if j < chars.len() && is_ident(chars[j]) {
            continue;
        }
        j = skip_ws(chars, j);
        if !starts_with_word(chars, j, "SRID") {
            return None;
        }
        j += 4;
        if j < chars.len() && is_ident(chars[j]) {
            return None;
        }
        j = skip_ws(chars, j);
        let digits_at = j;
        while j < chars.len() && chars[j].is_ascii_digit() {
            j += 1;
        }
        if j == digits_at {
            return None;
        }
        if j < chars.len() && is_ident(chars[j]) {
            return None;
        }
        let digits: String = chars[digits_at..j].iter().collect();
        return Some((format!("{name}({digits})"), j));
    }
    None
}

fn starts_with_word(chars: &[char], i: usize, word: &str) -> bool {
    let w: Vec<char> = word.chars().collect();
    if i + w.len() > chars.len() {
        return false;
    }
    chars[i..i + w.len()]
        .iter()
        .zip(w.iter())
        .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

fn skip_ws(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    i
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_column_srid_outside_strings() {
        let sql = "CREATE TABLE t (p POINT SRID 4326, g GEOMETRY, note VARCHAR(32) DEFAULT 'POINT SRID 1')";
        let rewritten = rewrite_spatial_column_srid(sql);
        assert!(rewritten.contains("POINT(4326)"), "{rewritten}");
        assert!(rewritten.contains("'POINT SRID 1'"), "{rewritten}");
        assert!(rewritten.contains("GEOMETRY,"), "{rewritten}");
    }
}
