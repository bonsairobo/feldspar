use super::{Change, ChunkDbKey, EncodedChanges, OwnedArchivedChange};
use crate::CompressedChunk;

use sled::transaction::{
    ConflictableTransactionError, TransactionalTree, UnabortableTransactionError,
};
use sled::Tree;
use std::collections::{BTreeMap, BTreeSet};

pub fn open_backup_tree(map_name: &str, db: &sled::Db) -> sled::Result<(Tree, BackupKeyCache)> {
    let tree = db.open_tree(format!("{}-backup", map_name))?;
    let mut keys = BTreeSet::default();
    for iter_result in tree.iter() {
        let (key_bytes, _) = iter_result?;
        keys.insert(ChunkDbKey::from_be_bytes(&key_bytes));
    }
    Ok((tree, BackupKeyCache { keys }))
}

pub fn write_changes_to_backup_tree(
    txn: &TransactionalTree,
    changes: EncodedChanges<CompressedChunk>,
) -> Result<(), UnabortableTransactionError> {
    for (key_bytes, change) in changes.changes.into_iter() {
        txn.insert(&key_bytes, change.take_bytes())?;
    }
    Ok(())
}

pub fn commit_backup(
    txn: &TransactionalTree,
    keys: &BackupKeyCache,
) -> Result<BTreeMap<ChunkDbKey, Change<CompressedChunk>>, ConflictableTransactionError<()>> {
    let mut changes = BTreeMap::default();
    for &key in keys.keys.iter() {
        if let Some(change) = txn.remove(&key.to_be_bytes())? {
            let archived_change = unsafe { OwnedArchivedChange::<CompressedChunk>::new(change) };
            changes.insert(key, archived_change.unarchive());
        } else {
            panic!("BUG: failed to get change backup for {:?}", key);
        }
    }
    Ok(changes)
}

pub fn clear_backup(
    txn: &TransactionalTree,
    keys: &BackupKeyCache,
) -> Result<(), UnabortableTransactionError> {
    for key in keys.keys.iter() {
        txn.remove(&key.to_be_bytes())?;
    }
    Ok(())
}

/// The set of keys currently stored in the backup tree. Equivalently: the set of keys that have been changed from the parent
/// version to the working version.
#[derive(Clone, Default)]
pub struct BackupKeyCache {
    /// [`BTreeSet`] is used for sorted iteration; which implies linear traversal over a sled tree.
    pub keys: BTreeSet<ChunkDbKey>,
}

// ████████╗███████╗███████╗████████╗
// ╚══██╔══╝██╔════╝██╔════╝╚══██╔══╝
//    ██║   █████╗  ███████╗   ██║
//    ██║   ██╔══╝  ╚════██║   ██║
//    ██║   ███████╗███████║   ██║
//    ╚═╝   ╚══════╝╚══════╝   ╚═╝

#[cfg(test)]
mod tests {
    use sled::transaction::TransactionError;

    use super::*;
    use crate::glam::IVec3;
    use crate::{Change, ChangeEncoder, Chunk};

    #[test]
    fn write_and_commit_backup() {
        let db = sled::Config::default().temporary(true).open().unwrap();
        let (tree, mut backup_keys) = open_backup_tree("mymap", &db).unwrap();

        assert!(backup_keys.keys.is_empty());

        let key1 = ChunkDbKey::new(1, IVec3::ZERO.into());
        let key2 = ChunkDbKey::new(2, IVec3::ONE.into());
        backup_keys.keys.insert(key1);
        backup_keys.keys.insert(key2);

        let mut encoder = ChangeEncoder::default();
        encoder.add_compressed_change(key1, Change::Remove);
        encoder.add_compressed_change(key2, Change::Insert(Chunk::default().compress()));
        let encoded_changes = encoder.encode();

        let _: Result<_, TransactionError<()>> = tree.transaction(|txn| {
            let _ = write_changes_to_backup_tree(txn, encoded_changes.clone())?;
            let reverse_changes = commit_backup(txn, &backup_keys)?;
            assert_eq!(
                reverse_changes,
                BTreeMap::from([
                    (key1, Change::Remove),
                    (key2, Change::Insert(Chunk::default().compress()))
                ])
            );
            Ok(())
        });
    }
}
