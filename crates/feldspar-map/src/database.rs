use crate::{CompressedChunk, Level};

use ilattice::morton::Morton3i32;
use rkyv::Archive;
use sled::Tree;
use std::{collections::BTreeMap, path::Path};

pub enum MapDatabaseError {}

#[derive(Archive, Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
#[archive_attr(derive(Eq, PartialEq, PartialOrd, Ord))]
pub struct Version {
    number: u64,
}

#[derive(Archive, Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
#[archive_attr(derive(Eq, PartialEq, PartialOrd, Ord))]
pub struct ChunkDbKey {
    level: Level,
    morton: Morton3i32,
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
    meta_tree: Tree,

    /// A map from [`ChunkDbKey`] to [`CompressedChunk`].
    bulk_tree: Tree,

    /// A map from [`Version`] to [`VersionChanges`].
    change_tree: Tree,

    /// A cache of the mapping from [`ChunkDbKey`] to [`Version`] for all keys that have been edited since the last version
    /// migration.
    ///
    /// If a DB reader does not find their key in this cache, then the current version for that key could only live in the
    /// `bulk_tree`. Otherwise the data lives in a [`VersionChanges`] found in the `change_tree`.
    key_version_cache: BTreeMap<ChunkDbKey, Version>,
}

impl MapDb {
    /// Opens the [`sled::Tree`]s that contain our database, initializing them with an empty map if they didn't already exist.
    pub fn open(meta_file_path: &Path) -> Result<Self, MapDatabaseError> {
        todo!()
    }

    /// Returns the [`Version`] that is seen by readers. Writer also make changes using this version as the parent.
    pub fn current_version(&self) -> Version {
        todo!()
    }

    /// Returns the [`Version`] that is completely and exactly represented by the bulk tree.
    pub fn bulk_version(&self) -> Version {
        todo!()
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

#[derive(Archive, Clone, Copy, Eq, PartialEq)]
#[archive_attr(derive(Eq, PartialEq))]
pub struct MapDbMetadata {
    pub current_version: Version,
    pub bulk_version: Version,
}

#[derive(Archive)]
pub enum Change {
    Insert(CompressedChunk),
    Remove,
}

#[derive(Archive)]
pub struct VersionChanges {
    /// The version immediately before this one.
    pub parent_version: Version,
    /// The full set of changes made between `parent_version` and this version.
    ///
    /// Kept in a btree map to be efficiently searchable by readers.
    pub changes: BTreeMap<ChunkDbKey, Change>,
}
