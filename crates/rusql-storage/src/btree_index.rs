//! B+Tree-backed secondary index (ordered map + row-id lists).

use std::collections::BTreeMap;

/// Secondary index mapping key values to heap row positions.
#[derive(Debug, Default, Clone)]
pub struct BTreeSecondaryIndex {
    tree: BTreeMap<String, Vec<u64>>,
}

impl BTreeSecondaryIndex {
    pub fn insert(&mut self, key: String, row_id: u64) {
        self.tree.entry(key).or_default().push(row_id);
    }

    pub fn lookup(&self, key: &str) -> &[u64] {
        static EMPTY: Vec<u64> = Vec::new();
        self.tree.get(key).map(|v| v.as_slice()).unwrap_or(&EMPTY)
    }

    pub fn len(&self) -> usize {
        self.tree.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tree.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_lookup() {
        let mut idx = BTreeSecondaryIndex::default();
        idx.insert("a".into(), 0);
        idx.insert("a".into(), 2);
        idx.insert("b".into(), 1);
        assert_eq!(idx.lookup("a"), &[0, 2]);
        assert_eq!(idx.lookup("b"), &[1]);
        assert!(idx.lookup("c").is_empty());
    }
}
