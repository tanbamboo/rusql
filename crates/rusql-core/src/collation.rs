//! MySQL collation-aware string compare and sort (M59).
//!
//! v1 implements `utf8mb4_unicode_ci` using Unicode NFD decomposition,
//! combining-mark stripping, case folding, and MySQL-style expansions (ß→ss, æ→ae, œ→oe).

use std::cmp::Ordering;
use unicode_normalization::UnicodeNormalization;

/// Supported collations for string compare/sort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Collation {
    #[default]
    Utf8Mb4UnicodeCi,
}

impl Collation {
    pub fn name(self) -> &'static str {
        match self {
            Self::Utf8Mb4UnicodeCi => "utf8mb4_unicode_ci",
        }
    }

    pub fn charset(self) -> &'static str {
        "utf8mb4"
    }

    pub fn from_name(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("utf8mb4_unicode_ci") {
            Some(Self::Utf8Mb4UnicodeCi)
        } else {
            None
        }
    }

    /// All collations advertised via `SHOW COLLATION`.
    pub fn supported() -> &'static [Self] {
        &[Self::Utf8Mb4UnicodeCi]
    }

    pub fn compare(self, a: &str, b: &str) -> Ordering {
        match self {
            Self::Utf8Mb4UnicodeCi => unicode_ci_sort_key(a).cmp(&unicode_ci_sort_key(b)),
        }
    }

    pub fn eq(self, a: &str, b: &str) -> bool {
        self.compare(a, b) == Ordering::Equal
    }
}

/// Default collation for utf8mb4 string columns.
pub const DEFAULT_COLLATION: Collation = Collation::Utf8Mb4UnicodeCi;

fn unicode_ci_sort_key(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            'ß' | 'ẞ' => out.extend_from_slice(b"ss"),
            'Æ' | 'æ' => out.extend_from_slice(b"ae"),
            'Œ' | 'œ' => out.extend_from_slice(b"oe"),
            c => {
                for d in c.nfd() {
                    if unicode_normalization::char::canonical_combining_class(d) != 0 {
                        continue;
                    }
                    for lc in d.to_lowercase() {
                        let mut buf = [0u8; 4];
                        out.extend_from_slice(lc.encode_utf8(&mut buf).as_bytes());
                    }
                }
            }
        }
    }
    out
}

/// Portable test corpus for `utf8mb4_unicode_ci` (≥10 strings, multi-byte included).
#[allow(dead_code)]
pub mod corpus {
    /// Pairs that must compare equal under `utf8mb4_unicode_ci`.
    pub const EQUAL_PAIRS: &[(&str, &str)] = &[
        ("apple", "Apple"),
        ("café", "cafe"),
        ("Müller", "muller"),
        ("straße", "strasse"),
        ("Résumé", "resume"),
        ("中文", "中文"),
        ("日本語", "日本語"),
        ("🦀rust", "🦀rust"),
        ("Æsir", "ÆSIR"),
        ("naïve", "naive"),
        ("Zürich", "zurich"),
        ("Ελληνικά", "ελληνικά"),
    ];

    /// Expected ascending sort order for `ORDER BY` (case/accent insensitive).
    pub const SORT_ASC: &[&str] = &[
        "apple",
        "Banana",
        "café",
        "Müller",
        "straße",
        "Zürich",
        "αβγ",
        "中文",
        "日本語",
        "🦀",
        "🦀rust",
    ];
}

#[cfg(test)]
mod tests {
    use super::corpus::{EQUAL_PAIRS, SORT_ASC};
    use super::*;

    #[test]
    fn equal_pairs_unicode_ci() {
        for (a, b) in EQUAL_PAIRS {
            assert!(DEFAULT_COLLATION.eq(a, b), "expected equal: {a:?} vs {b:?}");
        }
    }

    #[test]
    fn sort_order_matches_corpus() {
        let mut sorted = SORT_ASC.to_vec();
        sorted.sort_by(|a, b| DEFAULT_COLLATION.compare(a, b));
        assert_eq!(sorted, SORT_ASC);
    }

    #[test]
    fn case_insensitive_ordering() {
        assert_eq!(DEFAULT_COLLATION.compare("apple", "Banana"), Ordering::Less);
        assert!(DEFAULT_COLLATION.eq("test", "TEST"));
    }

    #[test]
    fn expansion_ss_and_ae() {
        assert!(DEFAULT_COLLATION.eq("straße", "strasse"));
        assert!(DEFAULT_COLLATION.eq("straße", "STRASSE"));
    }

    #[test]
    fn supported_collation_list() {
        assert_eq!(Collation::supported().len(), 1);
        assert_eq!(Collation::supported()[0].name(), "utf8mb4_unicode_ci");
    }
}
