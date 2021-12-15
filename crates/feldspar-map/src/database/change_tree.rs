use super::{Change, ChunkDbKey, Version};
use crate::CompressedChunk;

use rkyv::Archive;
use sled::transaction::TransactionError;
use sled::Tree;
use std::collections::BTreeMap;

#[derive(Archive)]
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

    pub async fn write_new_version(&self) {
        todo!()
    }
}
