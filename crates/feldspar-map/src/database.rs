mod bulk_tree;
mod change_encoder;
mod change_tree;
mod chunk_key;
mod meta_tree;

use bulk_tree::BulkTree;
use change_tree::{create_version, ChangeTree, VersionChanges};
use chunk_key::ChunkDbKey;
use meta_tree::{update_current_version, MetaTree};

use rkyv::{Archive, Deserialize, Serialize};
use sled::transaction::{TransactionError, Transactional};
use std::collections::BTreeMap;

use self::meta_tree::MapDbMetadata;

pub const FIRST_VERSION: Version = Version::new(0);

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

    pub fn into_sled_key(self) -> [u8; 8] {
        self.number.to_be_bytes()
    }
}

#[derive(Archive, Serialize)]
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

/// # Map Database Model
///
/// This database is effectively the backing store for a [`ChunkClipMap`](crate::ChunkClipMap). It supports CRUD operations on
/// [`CompressedChunk`]s as well as a versioned log of changes.
///
/// ## Implementation
///
/// All data is stored in [`sled::Tree`]s. One tree is used for the *bulk* [`Version`] of the map, and it stores most of the
/// [`CompressedChunk`] data that is relevant in the *current* version. The remainder of the data relevant to the current
/// version is found in a separate tree which stores the [`VersionChanges`] of all versions. All new changes are written
/// out-of-place in a new [`VersionChanges`] entry "ahead" of the bulk version.
///
/// ### Version Migration and Re-Bulking
///
/// While the bulk version differs from the current version, readers will be forced to consult a cached mapping from
/// [`ChunkDbKey`] to [`Version`] so that they can find the [`VersionChanges`] where the data lives.
///
/// In order to change the bulk version to match the current version, a sequence of [`VersionChanges`] must be applied to the
/// bulk [`sled::Tree`], and those changes are replaced by their *inverses* so that the bulk tree can also be merged in the
/// opposite direction. This process is referred to as *re-bulking*.
///
/// When changing the current version, the cached mapping from [`ChunkDbKey`] to [`Version`] must be updated accordingly so that
/// readers know where to find data.
pub struct MapDb {
    db: sled::Db,

    /// A map from `str` to arbitrary [`Archive`] data type. Currently contains:
    ///
    /// - "meta" -> [`MapDbMetadata`]
    meta_tree: MetaTree,

    /// A map from [`ChunkDbKey`] to [`CompressedChunk`].
    bulk_tree: BulkTree,

    /// A map from [`Version`] to [`VersionChanges`].
    change_tree: ChangeTree,

    /// A cache of the mapping from [`ChunkDbKey`] to [`Version`] for all keys that have been edited since the last version
    /// migration.
    ///
    /// If a DB reader does not find their key in this cache, then the current version for that key could only live in the
    /// `bulk_tree`. Otherwise the data lives in a [`VersionChanges`] found in the `change_tree`.
    key_version_cache: BTreeMap<ChunkDbKey, Version>,
}

impl MapDb {
    /// Opens the [`sled::Tree`]s that contain our database, initializing them with an empty map if they didn't already exist.
    pub fn open(db_name: &str, cache_capacity_bytes: usize) -> Result<Self, TransactionError> {
        let db = sled::Config::default()
            .cache_capacity(cache_capacity_bytes)
            .path(db_name)
            .open()?;
        let meta_tree = MetaTree::open(db_name, &db)?;
        let bulk_tree = BulkTree::open(db_name, &db)?;
        let change_tree = ChangeTree::open(db_name, &db)?;

        Ok(Self {
            db,
            meta_tree,
            bulk_tree,
            change_tree,
            key_version_cache: Default::default(),
        })
    }

    pub async fn flush(&self) -> sled::Result<usize> {
        self.db.flush_async().await
    }

    /// Returns the [`Version`] that is seen by readers. Writer also make changes using this version as the parent.
    pub fn cached_meta(&self) -> &MapDbMetadata {
        &self.meta_tree.cached_meta
    }

    pub fn create_version(&self, changes: &VersionChanges) -> Result<Version, TransactionError> {
        (&self.change_tree.tree, &self.meta_tree.tree).transaction(|(change_txn, meta_txn)| {
            let new_version = create_version(change_txn, changes)?;
            update_current_version(meta_txn, new_version);
            Ok(new_version)
        })
    }

    /// Sets the current version to `target_version`.
    ///
    /// After successful completion, readers will see all data at `target_version` and writers will create new
    /// [`VersionChanges`] entries parented by `target_version`.
    pub fn migrate_current_version(&self, target_version: Version) {
        todo!()
    }

    /// Applies all changes required to migrate the bulk tree from the bulk version to the current version.
    ///
    /// On successful completion, the bulk version will be set to the current version.
    pub fn rebulk(&self) {
        todo!()
    }
}
