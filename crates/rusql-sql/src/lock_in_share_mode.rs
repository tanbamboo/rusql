//! Rewrite MySQL `LOCK IN SHARE MODE` to `FOR SHARE` (sqlparser 0.53 has no alias).

const PHRASE: &[&str] = &["LOCK", "IN", "SHARE", "MODE"];

/// If `sql` contains `LOCK IN SHARE MODE`, rewrite those clauses to `FOR SHARE`.
pub fn rewrite_lock_in_share_mode(sql: &str) -> Option<String> {
    let bytes = sql.as_bytes();
    let mut out = String::with_capacity(sql.len());
    let mut i = 0;
    let mut changed = false;
    while i < bytes.len() {
        if let Some(next) = skip_literal_or_comment(sql, i) {
            out.push_str(&sql[i..next]);
            i = next;
            continue;
        }
        if ident_boundary_before(sql, i) {
            if let Some(end) = match_keyword_phrase(sql, i, PHRASE) {
                out.push_str("FOR SHARE");
                i = end;
                changed = true;
                continue;
            }
        }
        match sql[i..].chars().next() {
            Some(ch) => {
                out.push(ch);
                i += ch.len_utf8();
            }
            None => break,
        }
    }
    changed.then_some(out)
}

fn match_keyword_phrase(sql: &str, start: usize, words: &[&str]) -> Option<usize> {
    let bytes = sql.as_bytes();
    let mut i = start;
    for (n, word) in words.iter().enumerate() {
        if n > 0 {
            let mut saw_ws = false;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                saw_ws = true;
                i += 1;
            }
            if !saw_ws {
                return None;
            }
        }
        if i + word.len() > sql.len() || !sql[i..i + word.len()].eq_ignore_ascii_case(word) {
            return None;
        }
        i += word.len();
    }
    if i < bytes.len() {
        let next = bytes[i];
        if next.is_ascii_alphanumeric() || next == b'_' {
            return None;
        }
    }
    Some(i)
}

fn ident_boundary_before(sql: &str, i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let prev = sql.as_bytes()[i - 1];
    !prev.is_ascii_alphanumeric() && prev != b'_'
}

fn skip_literal_or_comment(sql: &str, i: usize) -> Option<usize> {
    let bytes = sql.as_bytes();
    if i >= bytes.len() {
        return None;
    }
    match bytes[i] {
        b'\'' | b'"' | b'`' => Some(skip_quoted(sql, i, bytes[i])),
        b'#' => Some(skip_line(sql, i + 1)),
        b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => Some(skip_line(sql, i + 2)),
        b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => Some(skip_block_comment(sql, i + 2)),
        _ => None,
    }
}

fn skip_quoted(sql: &str, start: usize, quote: u8) -> usize {
    let bytes = sql.as_bytes();
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if bytes[i] == quote {
            if i + 1 < bytes.len() && bytes[i + 1] == quote {
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    sql.len()
}

fn skip_line(sql: &str, mut i: usize) -> usize {
    let bytes = sql.as_bytes();
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    i
}

fn skip_block_comment(sql: &str, mut i: usize) -> usize {
    let bytes = sql.as_bytes();
    while i + 1 < bytes.len() {
        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
            return i + 2;
        }
        i += 1;
    }
    sql.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_lock_in_share_mode_to_for_share() {
        assert_eq!(
            rewrite_lock_in_share_mode("SELECT id FROM t LOCK IN SHARE MODE").as_deref(),
            Some("SELECT id FROM t FOR SHARE")
        );
        assert_eq!(
            rewrite_lock_in_share_mode("select id from t lock in share mode").as_deref(),
            Some("select id from t FOR SHARE")
        );
        assert_eq!(
            rewrite_lock_in_share_mode("SELECT id FROM t LOCK  IN\nSHARE MODE").as_deref(),
            Some("SELECT id FROM t FOR SHARE")
        );
    }

    #[test]
    fn rewrite_skips_plain_select_and_literals() {
        assert!(rewrite_lock_in_share_mode("SELECT id FROM t").is_none());
        assert!(rewrite_lock_in_share_mode("SELECT id FROM t FOR SHARE").is_none());
        assert!(rewrite_lock_in_share_mode("SELECT 'LOCK IN SHARE MODE' FROM t").is_none());
    }
}
