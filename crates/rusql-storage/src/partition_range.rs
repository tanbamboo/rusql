//! RANGE partition heap routing (M152). See docs/en/architecture/adr-m152-range-partitioning.md.

use rusql_core::{RangePartitionMeta, TableMeta};

use crate::{column_index, ColumnAssignment, DeleteFilter, HeapEngine, Row, StorageError};

pub(crate) const PARTITION_KEY_SEP: char = '\u{001f}';

pub fn partition_heap_key(table: &str, partition: &str) -> String {
    format!("{table}{PARTITION_KEY_SEP}{partition}")
}

pub fn resolve_partition_for_row(meta: &TableMeta, row: &Row) -> Result<String, StorageError> {
    let rp = meta
        .range_partition
        .as_ref()
        .ok_or_else(|| StorageError::Message("table is not range partitioned".into()))?;
    let idx = column_index(meta, &rp.key_column)?;
    let raw = row.get(idx).map(|s| s.as_str()).unwrap_or("");
    let value: i64 = raw.trim().parse().map_err(|_| {
        StorageError::Message(rusql_i18n::messages::sql_range_partition_bound_not_int())
    })?;
    rp.partition_for_i64(value)
        .map(|s| s.to_string())
        .ok_or_else(|| {
            StorageError::Message(rusql_i18n::messages::sql_no_partition_for_given_value())
        })
}

impl HeapEngine {
    pub(crate) fn range_partition_meta(&self, table: &str) -> Option<&RangePartitionMeta> {
        self.meta
            .get(table)
            .and_then(|m| m.range_partition.as_ref())
    }

    pub(crate) fn partition_data_keys(&self, table: &str) -> Result<Vec<String>, StorageError> {
        let meta = self
            .meta
            .get(table)
            .ok_or_else(|| StorageError::table_not_found(table))?;
        if let Some(rp) = &meta.range_partition {
            Ok(rp
                .partitions
                .iter()
                .map(|p| partition_heap_key(table, &p.name))
                .collect())
        } else {
            Ok(vec![table.to_string()])
        }
    }

    pub(crate) fn scan_heap(
        &self,
        table: &str,
        only_partitions: Option<&[String]>,
    ) -> Result<Vec<Row>, StorageError> {
        let keys = self.partition_data_keys(table)?;
        let read_keys: Vec<_> = match only_partitions {
            Some(names) => keys
                .into_iter()
                .filter(|k| {
                    names.iter().any(|n| {
                        k.ends_with(&format!("{PARTITION_KEY_SEP}{n}"))
                            || k.split(PARTITION_KEY_SEP).last() == Some(n.as_str())
                    })
                })
                .collect(),
            None => keys,
        };
        let mut out = Vec::new();
        for key in read_keys {
            if let Some(rows) = self.tables.get(&key) {
                out.extend(rows.iter().cloned());
            }
        }
        Ok(out)
    }

    pub(crate) fn row_count_heap(
        &self,
        table: &str,
        only_partitions: Option<&[String]>,
    ) -> Result<u64, StorageError> {
        let keys = self.partition_data_keys(table)?;
        let mut total = 0u64;
        for key in keys {
            if let Some(names) = only_partitions {
                let part = key.split(PARTITION_KEY_SEP).next_back().unwrap_or("");
                if !names.iter().any(|n| n == part) {
                    continue;
                }
            }
            total += self.tables.get(&key).map(|r| r.len() as u64).unwrap_or(0);
        }
        Ok(total)
    }

    pub(crate) fn update_rows_partitioned(
        &mut self,
        table: &str,
        table_meta: &TableMeta,
        assignments: &[ColumnAssignment],
        filter: Option<DeleteFilter>,
    ) -> Result<u64, StorageError> {
        let mut col_indices = Vec::with_capacity(assignments.len());
        for a in assignments {
            col_indices.push((column_index(table_meta, &a.column)?, a.value.clone()));
        }
        let keys = self.partition_data_keys(table)?;
        let mut updated = 0u64;
        for key in keys {
            let Some(rows) = self.tables.get_mut(&key) else {
                continue;
            };
            for row in rows.iter_mut() {
                let matches = match &filter {
                    None => true,
                    Some(f) => {
                        let idx = column_index(table_meta, &f.column)?;
                        row.get(idx).map(|v| v == &f.value).unwrap_or(false)
                    }
                };
                if !matches {
                    continue;
                }
                for &(idx, ref val) in &col_indices {
                    if idx >= row.len() {
                        return Err(StorageError::Message(format!(
                            "column index out of range for table '{table}'"
                        )));
                    }
                    row[idx] = val.clone();
                }
                updated += 1;
            }
        }
        Ok(updated)
    }

    pub(crate) fn drop_partition_heaps(&mut self, table: &str) {
        let keys: Vec<_> = self
            .tables
            .keys()
            .filter(|k| k.starts_with(table) && k.contains(PARTITION_KEY_SEP))
            .cloned()
            .collect();
        for k in keys {
            self.tables.remove(&k);
        }
    }
}

#[cfg(test)]
mod tests {
    use rusql_core::{
        ColumnDef, RangePartitionBound, RangePartitionDef, RangePartitionMeta, TableMeta,
    };

    use super::*;
    use crate::{HeapEngine, StorageEngine};

    fn range_table() -> TableMeta {
        TableMeta {
            name: "t".into(),
            schema: "rusql".into(),
            columns: vec![ColumnDef::new("id", "INT")],
            range_partition: Some(RangePartitionMeta {
                key_column: "id".into(),
                partitions: vec![
                    RangePartitionDef {
                        name: "p0".into(),
                        less_than: RangePartitionBound::Value(10),
                    },
                    RangePartitionDef {
                        name: "p1".into(),
                        less_than: RangePartitionBound::MaxValue,
                    },
                ],
            }),
            ..Default::default()
        }
    }

    #[test]
    fn partition_range_insert_routes_to_bucket() {
        let mut eng = HeapEngine::new();
        let meta = range_table();
        eng.create_table(meta).unwrap();
        eng.insert("t", vec!["5".into()]).unwrap();
        eng.insert("t", vec!["15".into()]).unwrap();
        let p0 = partition_heap_key("t", "p0");
        let p1 = partition_heap_key("t", "p1");
        assert_eq!(eng.scan(&p0).unwrap().len(), 1);
        assert_eq!(eng.scan(&p1).unwrap().len(), 1);
    }

    #[test]
    fn partition_range_prune_scans_one_partition() {
        let mut eng = HeapEngine::new();
        eng.create_table(range_table()).unwrap();
        eng.insert("t", vec!["5".into()]).unwrap();
        eng.insert("t", vec!["15".into()]).unwrap();
        let pruned = eng.scan_heap("t", Some(&["p0".into()])).unwrap();
        assert_eq!(pruned.len(), 1);
        assert_eq!(pruned[0][0], "5");
    }
}
