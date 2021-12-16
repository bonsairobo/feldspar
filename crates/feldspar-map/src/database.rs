mod bulk_tree;
mod change_encoder;
mod change_tree;
mod meta_tree;

use bulk_tree::BulkTree;
use change_tree::{ArchivedVersionChanges, ChangeTree};
use meta_tree::MetaTree;

use crate::glam::IVec3;
use crate::Level;

use core::ops::RangeInclusive;
use ilattice::prelude::{Bounded, Extent, Morton3i32};
use rkyv::{Archive, Deserialize, Serialize};
use sled::transaction::TransactionError;
use std::collections::BTreeMap;

use self::meta_tree::MapDbMetadata;

#[derive(
    Archive, Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, PartialOrd, Ord, Serialize,
)]
#[archive_attr(derive(Eq, PartialEq, PartialOrd, Ord))]
pub struct Version {
    pub number: u64,
}

impl Version {
    pub fn new(number: u64) -> Self {
        Self { number }
    }
}

#[derive(Archive, Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
#[archive_attr(derive(Eq, PartialEq, PartialOrd, Ord))]
pub struct ChunkDbKey {
    level: Level,
    morton: Morton3i32,
}

impl ChunkDbKey {
    pub fn new(level: Level, morton: Morton3i32) -> Self {
        Self { level, morton }
    }

    /// We implement this manually (without rkyv) so we have control over the [`Ord`] as interpreted by [`sled`].
    ///
    /// 13 bytes total per key, 1 for LOD and 12 for the morton code. Although a [`Morton3i32`] uses a u128, it only actually
    /// uses the least significant 96 bits (12 bytes).
    pub fn to_be_bytes(&self) -> [u8; 13] {
        let mut bytes = [0; 13];
        bytes[0] = self.level;
        bytes[1..].copy_from_slice(&self.morton.0.to_be_bytes()[4..]);
        bytes
    }

    pub fn from_be_bytes(bytes: &[u8]) -> Self {
        let level = bytes[0];
        // The most significant 4 bytes of the u128 are not used.
        let mut morton_bytes = [0; 16];
        morton_bytes[4..16].copy_from_slice(&bytes[1..]);
        let morton_int = u128::from_be_bytes(morton_bytes);
        Self::new(level, Morton3i32(morton_int))
    }

    pub fn extent_range(level: u8, extent: Extent<IVec3>) -> RangeInclusive<Self> {
        let min_morton = Morton3i32::from(extent.minimum);
        let max_morton = Morton3i32::from(extent.max());
        Self::new(level, min_morton)..=Self::new(level, max_morton)
    }

    pub fn min_key(level: u8) -> Self {
        Self::new(level, Morton3i32::from(IVec3::MIN))
    }

    pub fn max_key(level: u8) -> Self {
        Self::new(level, Morton3i32::from(IVec3::MAX))
    }
}

#[derive(Archive)]
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
    pub fn open(db_name: &str) -> Result<Self, TransactionError> {
        let db = sled::Config::default().path(db_name).open()?;
        let meta_tree = MetaTree::open(db_name, &db)?;
        let bulk_tree = BulkTree::open(db_name, &db)?;
        let change_tree = ChangeTree::open(db_name, &db)?;

        Ok(Self {
            meta_tree,
            bulk_tree,
            change_tree,
            key_version_cache: Default::default(),
        })
    }

    /// Returns the [`Version`] that is seen by readers. Writer also make changes using this version as the parent.
    pub fn cached_meta(&self) -> &MapDbMetadata {
        self.meta_tree.cached_meta()
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

    pub fn create_version(
        &self,
        changes: ArchivedVersionChanges,
    ) -> Result<Self, TransactionError> {
        todo!()
    }
}
