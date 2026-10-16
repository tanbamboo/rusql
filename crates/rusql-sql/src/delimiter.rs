//! Client `DELIMITER` handling for COM_QUERY batches (mysql CLI compatibility).
//!
//! The official mysql client treats `DELIMITER` locally; some scripts still send
//! `DELIMITER` lines over the wire. rusql accepts them as session delimiter changes
//! and splits subsequent statements on the active delimiter (string-literal aware).

const DEFAULT_DELIMITER: &str = ";";

/// Default statement delimiter for a new or reset session.
pub fn default_client_delimiter() -> String {
    DEFAULT_DELIMITER.to_string()
}

/// True when this SQL should be split/processed with the client delimiter protocol.
pub fn uses_delimiter_protocol(sql: &str, session_delimiter: &str) -> bool {
    session_delimiter != DEFAULT_DELIMITER || contains_delimiter_command(sql)
}

/// Split `sql` into executable statements, updating `session_delimiter` for `DELIMITER` lines.
///
/// When the session delimiter is `;` and the batch contains no `DELIMITER` command, returns
/// a single element (unchanged `sql`) so existing sqlparser multi-statement behavior is preserved.
pub fn expand_com_query_batch(sql: &str, session_delimiter: &mut String) -> Vec<String> {
    if !uses_delimiter_protocol(sql, session_delimiter) {
        return vec![sql.to_string()];
    }

    let mut out = Vec::new();
    let mut rest = sql;
    let mut active = session_delimiter.clone();

    while !rest.is_empty() {
        if let Some((new_del, consumed)) = take_leading_delimiter_line(rest) {
            active = new_del;
            rest = &rest[consumed..];
            continue;
        }
        let trimmed_start = rest.trim_start();
        if trimmed_start.is_empty() {
            break;
        }
        let skip = rest.len() - trimmed_start.len();
        let (stmt, consumed) = read_until_delimiter(trimmed_start, 0, &active);
        let advance = skip + consumed;
        rest = if advance >= rest.len() {
            ""
        } else {
            &rest[advance..]
        };
        let trimmed = stmt.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(new_del) = parse_delimiter_command(trimmed) {
            active = new_del;
            continue;
        }
        out.push(trimmed.to_string());
    }

    *session_delimiter = active;
    out
}

/// If `sql` is a client `DELIMITER` command, return the new delimiter string (may be empty).
pub fn parse_delimiter_command(sql: &str) -> Option<String> {
    let start = sql.find(|c: char| !c.is_whitespace())?;
    let rest = &sql[start..];
    const KW: &str = "DELIMITER";
    if rest.len() < KW.len() || !rest[..KW.len()].eq_ignore_ascii_case(KW) {
        return None;
    }
    match rest[KW.len()..].chars().next() {
        None => Some(String::new()),
        Some(c) if c.is_whitespace() => Some(rest[KW.len()..].trim_start().to_string()),
        Some(_) => None,
    }
}

fn contains_delimiter_command(sql: &str) -> bool {
    for line in sql.split(['\n', '\r']) {
        if parse_delimiter_command(line.trim()).is_some() {
            return true;
        }
    }
    parse_delimiter_command(sql.trim()).is_some()
}

/// If `sql` starts (after whitespace) with a `DELIMITER` line, return `(new_delimiter, bytes_consumed)`.
fn take_leading_delimiter_line(sql: &str) -> Option<(String, usize)> {
    let bytes = sql.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    while i < len && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= len {
        return None;
    }
    let line_start = i;
    while i < len && bytes[i] != b'\n' && bytes[i] != b'\r' {
        i += 1;
    }
    let line = sql[line_start..i].trim();
    let new_del = parse_delimiter_command(line)?;
    while i < len && (bytes[i] == b'\n' || bytes[i] == b'\r') {
        i += 1;
    }
    Some((new_del, i))
}

/// Read from `pos` until the next `delimiter` (outside string/backtick literals), or EOF.
fn read_until_delimiter(sql: &str, pos: usize, delimiter: &str) -> (String, usize) {
    if pos >= sql.len() {
        return (String::new(), 0);
    }
    if delimiter.is_empty() {
        return (sql[pos..].to_string(), sql.len() - pos);
    }
    let bytes = sql.as_bytes();
    let len = bytes.len();
    let mut i = pos;
    let mut quote: Option<u8> = None;

    while i < len {
        let b = bytes[i];
        if let Some(q) = quote {
            if b == q && (i == 0 || bytes[i - 1] != b'\\') {
                quote = None;
            }
            i += 1;
            continue;
        }
        if b == b'\'' || b == b'"' {
            quote = Some(b);
            i += 1;
            continue;
        }
        if b == b'`' {
            i += 1;
            while i < len && bytes[i] != b'`' {
                i += 1;
            }
            if i < len {
                i += 1;
            }
            continue;
        }
        if sql[i..].starts_with(delimiter) {
            let end = i;
            return (sql[pos..end].to_string(), i + delimiter.len() - pos);
        }
        i += 1;
    }
    (sql[pos..].to_string(), len - pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delimiter_stub_parses_command() {
        assert_eq!(
            parse_delimiter_command("DELIMITER //").as_deref(),
            Some("//")
        );
        assert_eq!(
            parse_delimiter_command("delimiter $$").as_deref(),
            Some("$$")
        );
        assert!(parse_delimiter_command("SELECT 1").is_none());
    }

    #[test]
    fn delimiter_stub_does_not_split_semicolon_in_strings() {
        let mut delim = default_client_delimiter();
        let batch = expand_com_query_batch(
            "DELIMITER //\nCREATE PROCEDURE p() BEGIN SELECT ';'; END//\nDELIMITER ;\n",
            &mut delim,
        );
        assert_eq!(delim, ";");
        assert_eq!(batch.len(), 1);
        assert!(batch[0].contains("SELECT ';'"));
    }

    #[test]
    fn delimiter_stub_expands_procedure_script() {
        let mut delim = default_client_delimiter();
        let script = "DELIMITER //\nCREATE PROCEDURE sp_m172() BEGIN SELECT 1; END//\nDELIMITER ;\nCALL sp_m172()";
        let parts = expand_com_query_batch(script, &mut delim);
        assert_eq!(delim, ";");
        assert_eq!(parts.len(), 2);
        assert!(parts[0].starts_with("CREATE PROCEDURE sp_m172"));
        assert_eq!(parts[1], "CALL sp_m172()");
    }

    #[test]
    fn delimiter_stub_passthrough_without_delimiter_keyword() {
        let mut delim = default_client_delimiter();
        let sql = "SELECT 1; SELECT 2";
        let parts = expand_com_query_batch(sql, &mut delim);
        assert_eq!(parts, vec![sql.to_string()]);
    }

    #[test]
    fn delimiter_stub_custom_delimiter_persists_across_calls() {
        let mut delim = default_client_delimiter();
        let first = expand_com_query_batch("DELIMITER //", &mut delim);
        assert!(first.is_empty());
        assert_eq!(delim, "//");
        let second = expand_com_query_batch("SELECT 1//", &mut delim);
        assert_eq!(second, vec!["SELECT 1".to_string()]);
        assert_eq!(delim, "//");
    }
}
