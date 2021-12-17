use super::{ChunkDbKey, VersionChanges};
use crate::{Change, EncodedChanges, SmallKeyHashSet};

use sled::transaction::{TransactionalTree, UnabortableTransactionError};
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

pub fn write_changes_to_backup_tree(
    txn: &TransactionalTree,
    changes: EncodedChanges,
) -> Result<(), UnabortableTransactionError> {
    for (key_bytes, change) in changes.changes.into_iter() {
        let key = ChunkDbKey::from_be_bytes(&key_bytes);
        match change {
            Change::Insert(value) => {
                txn.insert(&key_bytes, value)?;
            }
            Change::Remove => {
                txn.remove(&key_bytes)?;
            }
        }
    }
    Ok(())
}

pub fn commit_backup(
    txn: &TransactionalTree,
    keys: &BackupKeyCache,
) -> Result<VersionChanges, UnabortableTransactionError> {
    todo!()
}

/// The set of keys currently stored in the backup tree. Equivalently: the set of keys that have been changed from the parent
/// version to the working version.
#[derive(Clone, Default)]
pub struct BackupKeyCache {
    pub keys: SmallKeyHashSet<ChunkDbKey>,
}
