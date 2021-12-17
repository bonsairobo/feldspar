mod backup_tree;
mod change_encoder;
mod chunk_key;
mod meta_tree;
mod version_tree;
mod working_tree;

pub use chunk_key::ChunkDbKey;

use backup_tree::{archive_parent_version, open_backup_tree, BackupKeyCache};
use meta_tree::open_meta_tree;
use version_tree::{create_version, get_archived_version, open_version_tree, VersionChanges};
use working_tree::open_working_tree;

use rkyv::{Archive, Deserialize, Serialize};
use sled::transaction::TransactionError;
use sled::Tree;

use self::meta_tree::MapDbMetadata;

#[derive(
    Archive, Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, PartialOrd, Ord, Serialize,
)]
#[archive_attr(derive(Debug, Eq, PartialEq, PartialOrd, Ord))]
pub struct Version {
    pub number: u64,
}

impl Version {
    pub const fn new(number: u64) -> Self {
        Self { number }
    }

    pub const fn into_sled_key(self) -> [u8; 8] {
        self.number.to_be_bytes()
    }
}

#[derive(Archive, Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum Change<T> {
    Insert(T),
    Remove,
}

impl<T> Change<T> {
    pub fn map<S>(self, mut f: impl FnMut(T) -> S) -> Change<S> {
        match self {
            Change::Insert(x) => Change::Insert(f(x)),
            Change::Remove => Change::Remove,
        }
    }
}

/// # Map Database
///
/// This database is effectively the backing store for a [`ChunkClipMap`](crate::ChunkClipMap). It supports CRUD operations on
/// [`CompressedChunk`]s as well as a versioned log of changes.
///
/// ## Implementation
///
/// All data is stored in three [`sled::Tree`]s.
///
/// ### Working Tree
///
/// One tree is used for the *working* [`Version`] of the map, and it stores all of the
/// [`CompressedChunk`](crate::CompressedChunk) data for the working version. All new changes are written to this tree.
///
/// ### Backup Tree
///
/// As new changes are written, the old values are moved into the "backup tree." The backup tree is just a persistent buffer
/// that eventually gets archived when the working version is cut.
///
/// ### Version Tree
///
/// Archived versions get an entry in the "version tree." This stores an actual tree structure where each node has a parent
/// version (except for the root version). To "revert" to a parent version, all of the backed up values must be re-applied in
/// reverse order, while the corresponding newer values are archived. By transitivity, any archived version can be reached from
/// the current working version.
pub struct MapDb {
    meta_tree: Tree,
    working_tree: Tree,
    backup_tree: Tree,
    version_tree: Tree,

    /// HACK: We only have this type to work around sled's lack of transactional iteration. When archiving a version, we iterate
    /// over this set of keys and put the entries into the archive.
    backup_key_cache: BackupKeyCache,
    // Zero-copy isn't super important for this tiny struct, so we just copy it for convenience.
    cached_meta: MapDbMetadata,
}

impl MapDb {
    /// Opens the database. On first open, a single working version will be created with no parent version.
    pub fn open(db: &sled::Db, db_name: &str) -> Result<Self, TransactionError> {
        let (meta_tree, cached_meta) = open_meta_tree(db_name, db)?;
        let version_tree = open_version_tree(db_name, db)?;
        let (backup_tree, backup_key_cache) = open_backup_tree(db_name, db)?;
        let working_tree = open_working_tree(db_name, db)?;

        Ok(Self {
            meta_tree,
            working_tree,
            backup_tree,
            version_tree,
            backup_key_cache,
            cached_meta,
        })
    }

    pub fn cached_meta(&self) -> &MapDbMetadata {
        &self.cached_meta
    }

    /// Archives the backup tree entries into a [`VersionChanges`] that gets serialized and stored in the version archive tree
    /// with the current working [`Version`]. A new working version is generated.
    pub fn cut_working_version(&mut self) -> Result<(), TransactionError> {
        todo!()
    }

    /// Sets the parent version to `parent_version` and generates a new (empty) working child version.
    pub fn branch_from_version(&self, parent_version: Version) {
        todo!()
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
    use crate::glam::IVec3;
    use crate::Chunk;
    use std::collections::BTreeMap;
}
