use super::{
    ArchivedChange, ArchivedChangeIVec, ArchivedIVec, BackupKeyCache, Change, ChunkDbKey,
    EncodedChanges,
};
use crate::CompressedChunk;

use sled::transaction::{TransactionalTree, UnabortableTransactionError};
use sled::{IVec, Tree};

pub fn open_working_tree(map_name: &str, db: &sled::Db) -> sled::Result<Tree> {
    db.open_tree(format!("{}-working", map_name))
}

/// Inserts any previously unseen entries from `changes` into the backup tree (`txn`) and returns the [`EncodedChanges`] that
/// can reverse the transformation.
pub fn write_changes_to_working_tree(
    txn: &TransactionalTree,
    backup_key_cache: &BackupKeyCache,
    changes: EncodedChanges<CompressedChunk>,
) -> Result<EncodedChanges<CompressedChunk>, UnabortableTransactionError> {
    let mut reverse_changes = Vec::with_capacity(changes.changes.len());
    let remove_bytes = unsafe {
        ArchivedIVec::new(IVec::from(
            Change::<CompressedChunk>::serialize_remove::<16>().as_ref(),
        ))
    };
    for (key_bytes, change) in changes.changes.into_iter() {
        let old_value = match change.as_ref() {
            ArchivedChange::Insert(_) => txn.insert(&key_bytes, change.take_bytes())?,
            ArchivedChange::Remove => txn.remove(&key_bytes)?,
        };

        let key = ChunkDbKey::from_sled_key(&key_bytes);
        if backup_key_cache.keys.contains(&key) {
            // We only want the oldest changes for the backup version.
            continue;
        }

        if let Some(old_value) = old_value {
            reverse_changes.push((key_bytes, unsafe {
                ArchivedChangeIVec::<CompressedChunk>::new(old_value)
            }));
        } else {
            reverse_changes.push((key_bytes, remove_bytes.clone()));
        }
    }
    Ok(EncodedChanges {
        changes: reverse_changes,
    })
}
