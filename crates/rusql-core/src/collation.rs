//! MySQL collation-aware string compare and sort (M59 + M62).
//!
//! - `utf8mb4_unicode_ci`: NFD + combining-mark strip + case fold + MySQL expansions (ß→ss, æ→ae, œ→oe)
//! - `utf8mb4_0900_ai_ci`: accent/case insensitive without German expansions (UCA 9.0 MVP approximation)

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

/// Supported collations for string compare/sort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Collation {
    /// MySQL 8.0 default collation (M62).
    Utf8Mb4_0900AiCi,
    #[default]
    Utf8Mb4UnicodeCi,
}

impl Collation {
    pub fn name(self) -> &'static str {
        match self {
            Self::Utf8Mb4_0900AiCi => "utf8mb4_0900_ai_ci",
            Self::Utf8Mb4UnicodeCi => "utf8mb4_unicode_ci",
        }
    }

    pub fn charset(self) -> &'static str {
        "utf8mb4"
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "utf8mb4_0900_ai_ci" => Some(Self::Utf8Mb4_0900AiCi),
            "utf8mb4_unicode_ci" => Some(Self::Utf8Mb4UnicodeCi),
            _ => None,
        }
    }

    /// All collations advertised via `SHOW COLLATION`.
    pub fn supported() -> &'static [Self] {
        &[Self::Utf8Mb4_0900AiCi, Self::Utf8Mb4UnicodeCi]
    }

    pub fn compare(self, a: &str, b: &str) -> Ordering {
        match self {
            Self::Utf8Mb4_0900AiCi => ai_ci_0900_sort_key(a).cmp(&ai_ci_0900_sort_key(b)),
            Self::Utf8Mb4UnicodeCi => unicode_ci_sort_key(a).cmp(&unicode_ci_sort_key(b)),
        }
    }

    pub fn eq(self, a: &str, b: &str) -> bool {
        self.compare(a, b) == Ordering::Equal
    }
}

/// Default collation for utf8mb4 string columns (rusql catalog default; MySQL 8.0 uses `utf8mb4_0900_ai_ci`).
pub const DEFAULT_COLLATION: Collation = Collation::Utf8Mb4UnicodeCi;

fn ai_ci_0900_sort_key(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for ch in s.chars() {
        for d in ch.nfd() {
            if unicode_normalization::char::canonical_combining_class(d) != 0 {
                continue;
            }
            for lc in d.to_lowercase() {
                let mut buf = [0u8; 4];
                out.extend_from_slice(lc.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
    out
}

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

    /// `utf8mb4_0900_ai_ci` corpus (M62).
    pub mod utf8mb4_0900_ai_ci {
        use super::super::Collation;

        pub const COLLATION: Collation = Collation::Utf8Mb4_0900AiCi;

        /// Pairs equal under `utf8mb4_0900_ai_ci`.
        pub const EQUAL_PAIRS: &[(&str, &str)] = &[
            ("apple", "Apple"),
            ("café", "cafe"),
            ("Müller", "muller"),
            ("Résumé", "resume"),
            ("naïve", "naive"),
            ("Zürich", "zurich"),
            ("test", "TEST"),
            ("hello", "HELLO"),
            ("中文", "中文"),
            ("日本語", "日本語"),
            ("alpha", "ALPHA"),
            ("bravo", "Bravo"),
        ];

        /// Pairs that must **not** compare equal (differs from `utf8mb4_unicode_ci`).
        pub const NOT_EQUAL_PAIRS: &[(&str, &str)] = &[("straße", "strasse"), ("Æsir", "aesir")];

        pub const SORT_ASC: &[&str] = &[
            "alpha", "ALPHA", "apple", "Apple", "Banana", "bravo", "Bravo", "café", "charlie",
            "delta", "echo", "foxtrot",
        ];
    }
}

#[cfg(test)]
mod tests {
    use super::corpus::utf8mb4_0900_ai_ci::{
        COLLATION as CI_0900, EQUAL_PAIRS as EQ_0900, NOT_EQUAL_PAIRS, SORT_ASC as SORT_0900,
    };
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
    fn equal_pairs_0900_ai_ci() {
        for (a, b) in EQ_0900 {
            assert!(CI_0900.eq(a, b), "expected equal: {a:?} vs {b:?}");
        }
    }

    #[test]
    fn not_equal_pairs_0900_ai_ci() {
        for (a, b) in NOT_EQUAL_PAIRS {
            assert!(!CI_0900.eq(a, b), "expected not equal: {a:?} vs {b:?}");
        }
    }

    #[test]
    fn sort_order_0900_ai_ci() {
        let mut sorted = SORT_0900.to_vec();
        sorted.sort_by(|a, b| CI_0900.compare(a, b));
        assert_eq!(sorted, SORT_0900);
    }

    #[test]
    fn unicode_ci_differs_from_0900_on_expansion() {
        assert!(DEFAULT_COLLATION.eq("straße", "strasse"));
        assert!(!CI_0900.eq("straße", "strasse"));
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
        assert_eq!(Collation::supported().len(), 2);
        assert_eq!(Collation::supported()[0].name(), "utf8mb4_0900_ai_ci");
        assert_eq!(Collation::supported()[1].name(), "utf8mb4_unicode_ci");
    }
}
