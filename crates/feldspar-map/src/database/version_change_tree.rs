use super::{ArchivedIVec, Change, ChunkDbKey, EncodedChanges, Version};
use crate::chunk::CompressedChunk;
use crate::core::NoSharedAllocSerializer;
use crate::core::rkyv::ser::Serializer;
use crate::core::rkyv::{Archive, Deserialize, Serialize};

use sled::transaction::TransactionalTree;
use sled::{transaction::UnabortableTransactionError, Tree};
use std::collections::BTreeMap;

#[derive(Archive, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[archive(crate = "crate::core::rkyv")]
pub struct VersionChanges {
    /// The full set of changes made between `parent_version` and this version.
    ///
    /// Kept in a btree map to be efficiently searchable by readers of the archive.
    pub changes: BTreeMap<ChunkDbKey, Change<CompressedChunk>>,
}

impl VersionChanges {
    pub fn new(changes: BTreeMap<ChunkDbKey, Change<CompressedChunk>>) -> Self {
        Self { changes }
    }
}

impl From<&EncodedChanges<CompressedChunk>> for VersionChanges {
    fn from(changes: &EncodedChanges<CompressedChunk>) -> Self {
        Self {
            changes: BTreeMap::from_iter(
                changes
                    .changes
                    .iter()
                    .map(|(key, value)| (ChunkDbKey::from_sled_key(key), value.deserialize())),
            ),
        }
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
    let mut serializer = NoSharedAllocSerializer::<8192>::default();
    serializer.serialize_value(changes).unwrap();
    let changes_bytes = serializer.into_serializer().into_inner();
    txn.insert(&version.into_sled_key(), changes_bytes.as_ref())?;
    Ok(())
}

pub fn remove_archived_version(
    txn: &TransactionalTree,
    version: Version,
) -> Result<Option<ArchivedIVec<VersionChanges>>, UnabortableTransactionError> {
    let bytes = txn.remove(&version.into_sled_key())?;
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

    use crate::chunk::Chunk;
    use crate::core::glam::IVec3;
    use crate::core::rkyv::option::ArchivedOption;

    use sled::transaction::TransactionError;

    #[test]
    fn open_archive_and_get() {
        let db = sled::Config::default().temporary(true).open().unwrap();
        let tree = db.open_tree("mymap-changes").unwrap();
        let v0 = Version::new(0);

        let mut original_changes = BTreeMap::new();
        original_changes.insert(
            ChunkDbKey::new(1, IVec3::ZERO.into()),
            Change::Insert(Chunk::default().compress()),
        );
        original_changes.insert(ChunkDbKey::new(2, IVec3::ZERO.into()), Change::Remove);
        let changes = VersionChanges::new(original_changes.clone());

        let changes: Result<VersionChanges, TransactionError> = tree.transaction(|txn| {
            assert!(
                remove_archived_version(txn, v0).unwrap()
                    == ArchivedOption::<ArchivedIVec<VersionChanges>>::None
            );

            archive_version(txn, v0, &changes).unwrap();

            let owned_archive = remove_archived_version(txn, Version::new(0))?.unwrap();

            Ok(owned_archive.deserialize())
        });
        assert_eq!(changes.unwrap(), VersionChanges::new(original_changes));
    }
}
