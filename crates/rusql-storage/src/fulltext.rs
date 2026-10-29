//! Inverted FULLTEXT index (M197).
//!
//! Tokens are ASCII alphanumeric runs of at least [`FT_MIN_TOKEN_LEN`] characters,
//! lowercased. This is not an ngram parser and not InnoDB's ranking formula.

use std::collections::BTreeSet;

/// InnoDB `innodb_ft_min_token_size` default. Shorter runs are not indexed.
pub const FT_MIN_TOKEN_LEN: usize = 3;

/// Boolean-mode query after tokenization. Ranking is not an InnoDB score.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FulltextBooleanQuery {
    /// `+word` terms. Every one must occur.
    pub required: Vec<String>,
    /// `-word` terms. None of them may occur.
    pub excluded: Vec<String>,
    /// Bare words. Used only when `required` is empty: any one may match.
    pub optional: Vec<String>,
}

/// Boolean query text this slice does not evaluate (phrase, wildcard, grouping).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FulltextParseError;

/// Same column list, same order, ASCII case-insensitive names.
pub fn fulltext_columns_match(index_cols: &[String], query_cols: &[String]) -> bool {
    index_cols.len() == query_cols.len()
        && index_cols
            .iter()
            .zip(query_cols)
            .all(|(index_col, query_col)| index_col.eq_ignore_ascii_case(query_col))
}

/// Split `text` into index tokens. Duplicates within one document are dropped.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            cur.push(ch.to_ascii_lowercase());
        } else if !cur.is_empty() {
            push_token(&mut out, &cur);
            cur.clear();
        }
    }
    if !cur.is_empty() {
        push_token(&mut out, &cur);
    }
    out.sort();
    out.dedup();
    out
}

fn push_token(out: &mut Vec<String>, raw: &str) {
    if raw.chars().count() >= FT_MIN_TOKEN_LEN {
        out.push(raw.to_string());
    }
}

/// Parse `IN BOOLEAN MODE` query text. `+` / `-` / bare words only.
pub fn parse_boolean_query(input: &str) -> Result<FulltextBooleanQuery, FulltextParseError> {
    if input
        .chars()
        .any(|ch| matches!(ch, '"' | '*' | '(' | ')' | '<' | '>' | '~' | '@'))
    {
        return Err(FulltextParseError);
    }
    let mut required = Vec::new();
    let mut excluded = Vec::new();
    let mut optional = Vec::new();
    for raw in input.split_whitespace() {
        let (bucket, word) = if let Some(rest) = raw.strip_prefix('+') {
            (&mut required, rest)
        } else if let Some(rest) = raw.strip_prefix('-') {
            (&mut excluded, rest)
        } else {
            (&mut optional, raw)
        };
        bucket.extend(tokenize(word));
    }
    Ok(FulltextBooleanQuery {
        required,
        excluded,
        optional,
    })
}

/// Whether `text` satisfies a boolean query. A query with no usable tokens matches nothing.
pub fn document_matches(text: &str, query: &FulltextBooleanQuery) -> bool {
    if query.required.is_empty() && query.optional.is_empty() {
        return false;
    }
    let tokens: BTreeSet<String> = tokenize(text).into_iter().collect();
    if !query.required.is_empty() {
        if !query.required.iter().all(|token| tokens.contains(token)) {
            return false;
        }
    } else if !query.optional.iter().any(|token| tokens.contains(token)) {
        return false;
    }
    !query.excluded.iter().any(|token| tokens.contains(token))
}

/// Row ids whose postings satisfy `query`. `lookup` returns ids for one token.
pub fn matching_row_ids(
    lookup: impl Fn(&str) -> Vec<u64>,
    query: &FulltextBooleanQuery,
) -> Vec<u64> {
    if query.required.is_empty() && query.optional.is_empty() {
        return Vec::new();
    }
    let mut ids = if !query.required.is_empty() {
        let mut acc: Option<BTreeSet<u64>> = None;
        for token in &query.required {
            let set: BTreeSet<u64> = lookup(token).into_iter().collect();
            acc = Some(match acc {
                None => set,
                Some(prev) => prev.intersection(&set).copied().collect(),
            });
        }
        acc.unwrap_or_default()
    } else {
        let mut union = BTreeSet::new();
        for token in &query.optional {
            union.extend(lookup(token));
        }
        union
    };
    for token in &query.excluded {
        for id in lookup(token) {
            ids.remove(&id);
        }
    }
    ids.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_drops_short_and_splits_ascii() {
        assert_eq!(
            tokenize("Foo, bar!!"),
            vec!["bar".to_string(), "foo".to_string()]
        );
        assert!(tokenize("a to").is_empty());
    }

    #[test]
    fn boolean_plus_word_requires_token() {
        let query = parse_boolean_query("+foo").unwrap();
        assert!(document_matches("foo bar", &query));
        assert!(!document_matches("foobar", &query));
        assert!(!document_matches("bar", &query));
    }
}
