use super::{Change, ChunkDbKey, Version};
use crate::CompressedChunk;

use rkyv::ser::{serializers::AllocSerializer, Serializer};
use rkyv::{Archive, Serialize};
use sled::transaction::TransactionError;
use sled::Tree;
use std::collections::BTreeMap;

/// ## Perf Note
///
/// Readers will need to fetch the entire [`ArchivedVersionChanges`] at a time from `sled`, but ideally it will reside in cache
/// until those changes are rebulked.
#[derive(Archive, Serialize)]
pub struct VersionChanges {
    /// The version immediately before this one.
    pub parent_version: Version,
    /// The full set of changes made between `parent_version` and this version.
    ///
    /// Kept in a btree map to be efficiently searchable by readers.
    pub changes: BTreeMap<ChunkDbKey, Change<CompressedChunk>>,
}

/// A mapping from [`Version`] to [`VersionChanges`].
pub struct ChangeTree {
    tree: Tree,
}

impl ChangeTree {
    pub fn open(db_name: &str, db: &sled::Db) -> Result<Self, TransactionError> {
        let tree = db.open_tree(format!("{}-changes", db_name))?;
        Ok(Self { tree })
    }

    pub fn create_version(&self, changes: &VersionChanges) -> Result<Version, TransactionError> {
        let mut serializer = AllocSerializer::<8192>::default();
        serializer.serialize_value(changes).unwrap();
        let changes_bytes = serializer.into_serializer().into_inner();

        self.tree.transaction(|txn| {
            let new_version = Version::new(txn.generate_id()?);
            txn.insert(&new_version.into_sled_key(), changes_bytes.as_ref())?;
            Ok(new_version)
        })
    }
}
