use super::{ChunkDbKey, VersionChanges};
use crate::SmallKeyHashSet;

use sled::transaction::{TransactionError, TransactionalTree};
use sled::Tree;

pub fn open_backup_tree(map_name: &str, db: &sled::Db) -> sled::Result<(Tree, BackupKeyCache)> {
    let tree = db.open_tree(format!("{}-backup", map_name))?;
    let mut all_keys = SmallKeyHashSet::default();
    for iter_result in tree.iter() {
        let (key_bytes, _) = iter_result?;
        all_keys.insert(ChunkDbKey::from_be_bytes(&key_bytes));
    }
    Ok((tree, BackupKeyCache { keys: all_keys }))
}

pub fn archive_parent_version(
    txn: &TransactionalTree,
    keys: &BackupKeyCache,
) -> Result<VersionChanges, TransactionError> {
    todo!()
}

/// The set of keys currently stored in the backup tree.
#[derive(Default)]
pub struct BackupKeyCache {
    keys: SmallKeyHashSet<ChunkDbKey>,
}
