use super::{ArchivedIVec, Change, ChunkDbKey, Version};
use crate::CompressedChunk;

use rkyv::ser::{serializers::AllocSerializer, Serializer};
use rkyv::{archived_root, Archive, Serialize};
use sled::transaction::TransactionalTree;
use sled::{transaction::UnabortableTransactionError, IVec, Tree};
use std::collections::BTreeMap;

#[derive(Archive, Eq, PartialEq, Serialize)]
pub struct VersionChanges {
    /// The full set of changes made between `parent_version` and this version.
    ///
    /// Kept in a btree map to be efficiently searchable by readers.
    pub changes: BTreeMap<ChunkDbKey, Change<CompressedChunk>>,
}

impl VersionChanges {
    pub fn new(changes: BTreeMap<ChunkDbKey, Change<CompressedChunk>>) -> Self {
        Self { changes }
    }
}

pub fn open_version_change_tree(map_name: &str, db: &sled::Db) -> sled::Result<Tree> {
    db.open_tree(format!("{}-version-changes", map_name))
}

pub fn archive_version(
    txn: &TransactionalTree,
    version: Version,
    changes: &VersionChanges,
) -> Result<(), UnabortableTransactionError> {
    let mut serializer = AllocSerializer::<8192>::default();
    serializer.serialize_value(changes).unwrap();
    let changes_bytes = serializer.into_serializer().into_inner();
    txn.insert(&version.into_sled_key(), changes_bytes.as_ref())?;
    Ok(())
}

pub fn get_archived_version(
    txn: &TransactionalTree,
    version: Version,
) -> Result<Option<ArchivedIVec<VersionChanges>>, UnabortableTransactionError> {
    let bytes = txn.get(&version.into_sled_key())?;
    Ok(bytes.map(|b| unsafe { ArchivedIVec::<VersionChanges>::new(b) }))
}

// ████████╗███████╗███████╗████████╗
// ╚══██╔══╝██╔════╝██╔════╝╚══██╔══╝
//    ██║   █████╗  ███████╗   ██║
//    ██║   ██╔══╝  ╚════██║   ██║
//    ██║   ███████╗███████║   ██║
//    ╚═╝   ╚══════╝╚══════╝   ╚═╝

#[cfg(test)]
mod tests {
    use super::*;

    use rkyv::option::ArchivedOption;
    use sled::transaction::TransactionError;

    #[test]
    fn open_archive_and_get() {
        let db = sled::Config::default().temporary(true).open().unwrap();
        let tree = db.open_tree("mymap-changes").unwrap();
        let v0 = Version::new(0);

        let result: Result<(), TransactionError> = tree.transaction(|txn| {
            assert!(
                get_archived_version(txn, v0).unwrap()
                    == ArchivedOption::<ArchivedIVec<VersionChanges>>::None
            );

            let changes = VersionChanges::new(BTreeMap::new());
            archive_version(txn, v0, &changes).unwrap();

            let owned_archive = get_archived_version(txn, Version::new(0)).unwrap().unwrap();
            assert!(owned_archive.as_ref().changes.is_empty());

            Ok(())
        });
        result.unwrap();
    }
}
