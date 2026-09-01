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

    pub fn range(&self, low: &str, high: &str) -> Vec<u64> {
        self.tree
            .range(low.to_string()..=high.to_string())
            .flat_map(|(_, ids)| ids.iter().copied())
            .collect()
    }

    /// Row ids in key order, skipping `offset` and collecting at most `limit`.
    pub fn ordered_ids(&self, ascending: bool, offset: usize, limit: usize) -> Vec<u64> {
        let mut skipped = 0usize;
        let mut out = Vec::with_capacity(limit.min(128));
        let iter: Box<dyn Iterator<Item = (&String, &Vec<u64>)>> = if ascending {
            Box::new(self.tree.iter())
        } else {
            Box::new(self.tree.iter().rev())
        };
        for (_, ids) in iter {
            for &id in ids {
                if skipped < offset {
                    skipped += 1;
                    continue;
                }
                out.push(id);
                if out.len() >= limit {
                    return out;
                }
            }
        }
        out
    }

    pub fn remove(&mut self, key: &str, row_id: u64) {
        if let Some(ids) = self.tree.get_mut(key) {
            ids.retain(|&id| id != row_id);
            if ids.is_empty() {
                self.tree.remove(key);
            }
        }
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
