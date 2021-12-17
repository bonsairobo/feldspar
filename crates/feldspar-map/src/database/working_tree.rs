use super::BackupKeyCache;
use crate::{Change, ChunkDbKey, EncodedChanges};

use sled::transaction::{TransactionalTree, UnabortableTransactionError};
use sled::Tree;

pub fn open_working_tree(map_name: &str, db: &sled::Db) -> sled::Result<Tree> {
    db.open_tree(format!("{}-working", map_name))
}

/// Inserts any previously unseen entries from `changes` into the backup tree (`txn`) and returns the [`EncodedChanges`] that
/// can reverse the transformation.
pub fn write_changes_to_working_tree(
    txn: &TransactionalTree,
    backup_key_cache: &BackupKeyCache,
    changes: EncodedChanges,
) -> Result<EncodedChanges, UnabortableTransactionError> {
    let mut reverse_changes = Vec::with_capacity(changes.changes.len());
    for (key_bytes, change) in changes.changes.into_iter() {
        let old_value = match change {
            Change::Insert(value) => txn.insert(&key_bytes, value)?,
            Change::Remove => txn.remove(&key_bytes)?,
        };

        let key = ChunkDbKey::from_be_bytes(&key_bytes);
        if backup_key_cache.keys.contains(&key) {
            // We only want the oldest changes for the backup version.
            continue;
        }

        if let Some(old_value) = old_value {
            reverse_changes.push((key_bytes, Change::Insert(old_value)));
        } else {
            reverse_changes.push((key_bytes, Change::Remove));
        }
    }
    Ok(EncodedChanges {
        changes: reverse_changes,
    })
}
