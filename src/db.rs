use std::{path::Path, sync::Arc};

use anyhow::Result;
use rocksdb::{
    ColumnFamily, ColumnFamilyDescriptor, DBCompressionType, MergeOperands, Options,
    SliceTransform, DB,
};

struct Column {
    name: &'static str,
    prefix: Option<usize>,
}

#[derive(Clone)]
pub struct Database(pub(super) Arc<DB>);

impl Database {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let cf_names = [Column {
            name: "offers",
            prefix: None,
        }];

        let mut options = Options::default();
        options.create_if_missing(true);
        options.create_missing_column_families(true);
        options.prepare_for_bulk_load();

        let cf_descriptors: Vec<ColumnFamilyDescriptor> = cf_names
            .iter()
            .map(|column| {
                let mut cf_opts = Options::default();

                // Optimize index column families
                if let Some(prefix) = column.prefix {
                    cf_opts.set_prefix_extractor(SliceTransform::create_fixed_prefix(prefix));
                    cf_opts.set_memtable_prefix_bloom_ratio(0.1);
                }

                cf_opts.set_compression_type(DBCompressionType::Lz4);

                // Use different settings for coin data vs indexes
                if column.prefix.is_some() {
                    cf_opts.set_merge_operator_associative("test operator", concat_merge);
                } else {
                    cf_opts.set_bottommost_compression_type(DBCompressionType::Zstd);
                }

                ColumnFamilyDescriptor::new(column.name.to_string(), cf_opts)
            })
            .collect();

        // Open database with column families
        let db = DB::open_cf_descriptors(&options, path, cf_descriptors)?;

        Ok(Self(Arc::new(db)))
    }

    pub fn offer(&self, code: [u8; 12]) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get_cf(self.offers_cf(), code)?)
    }

    pub fn insert_offer(&self, code: [u8; 12], offer: Vec<u8>) -> Result<()> {
        self.0.put_cf(self.offers_cf(), code, offer)?;
        Ok(())
    }

    pub(super) fn offers_cf(&self) -> &ColumnFamily {
        self.0.cf_handle("offers").unwrap()
    }
}

fn concat_merge(
    _new_key: &[u8],
    existing_val: Option<&[u8]>,
    operands: &MergeOperands,
) -> Option<Vec<u8>> {
    let mut result: Vec<u8> = Vec::with_capacity(operands.len());
    if let Some(v) = existing_val {
        for e in v {
            result.push(*e)
        }
    }
    for op in operands {
        for e in op {
            result.push(*e)
        }
    }
    Some(result)
}
