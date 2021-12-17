use super::{Change, ChunkDbKey, Version};
use crate::CompressedChunk;

use rkyv::ser::{serializers::AllocSerializer, Serializer};
use rkyv::{archived_root, Archive, Serialize};
use sled::transaction::{ConflictableTransactionError, TransactionalTree};
use sled::{IVec, Tree};
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

impl VersionChanges {
    pub fn new(
        parent_version: Version,
        changes: BTreeMap<ChunkDbKey, Change<CompressedChunk>>,
    ) -> Self {
        Self {
            parent_version,
            changes,
        }
    }
}

pub fn open_change_tree(db_name: &str, db: &sled::Db) -> sled::Result<Tree> {
    db.open_tree(format!("{}-changes", db_name))
}

/// Non-blocking write.
pub fn create_version(
    txn: &TransactionalTree,
    changes: &VersionChanges,
) -> Result<Version, ConflictableTransactionError> {
    let mut serializer = AllocSerializer::<8192>::default();
    serializer.serialize_value(changes).unwrap();
    let changes_bytes = serializer.into_serializer().into_inner();

    let new_version = Version::new(txn.generate_id()?);
    txn.insert(&new_version.into_sled_key(), changes_bytes.as_ref())?;
    Ok(new_version)
}

/// This may need to block on IO and should probably run in an async task.
pub fn get_archived_version(
    txn: &TransactionalTree,
    version: Version,
) -> Result<Option<OwnedArchivedVersionChanges>, ConflictableTransactionError> {
    let bytes = txn.get(&version.into_sled_key())?;
    Ok(bytes.map(OwnedArchivedVersionChanges::new))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedArchivedVersionChanges {
    bytes: IVec,
}

impl OwnedArchivedVersionChanges {
    pub fn new(bytes: IVec) -> Self {
        Self { bytes }
    }
}

impl AsRef<ArchivedVersionChanges> for OwnedArchivedVersionChanges {
    fn as_ref(&self) -> &ArchivedVersionChanges {
        unsafe { archived_root::<VersionChanges>(self.bytes.as_ref()) }
    }
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
    use crate::{ArchivedVersion, FIRST_VERSION};

    use sled::transaction::TransactionError;

    #[test]
    fn open_create_and_get() {
        let db = sled::Config::default().temporary(true).open().unwrap();
        let tree = db.open_tree("mymap-changes").unwrap();

        let result: Result<(), TransactionError> = tree.transaction(|txn| {
            assert_eq!(get_archived_version(txn, FIRST_VERSION).unwrap(), None);

            let changes = VersionChanges::new(FIRST_VERSION, BTreeMap::new());
            create_version(txn, &changes).unwrap();

            let owned_archive = get_archived_version(txn, Version::new(0)).unwrap().unwrap();
            assert_eq!(
                owned_archive.as_ref().parent_version,
                ArchivedVersion {
                    number: FIRST_VERSION.number
                }
            );

            Ok(())
        });
        result.unwrap();
    }
}
