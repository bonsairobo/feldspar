mod backup_tree;
mod change_encoder;
mod chunk_key;
mod meta_tree;
mod version_change_tree;
mod version_graph_tree;
mod working_tree;

pub use change_encoder::*;
pub use chunk_key::ChunkDbKey;
pub use version_change_tree::VersionChanges;

use backup_tree::{
    clear_backup, commit_backup, open_backup_tree, write_changes_to_backup_tree, BackupKeyCache,
};
use meta_tree::{open_meta_tree, write_meta};
use version_change_tree::{archive_version, open_version_change_tree, remove_archived_version};
use version_graph_tree::{
    find_path_between_versions, link_version, open_version_graph_tree, VersionNode,
};
use working_tree::{open_working_tree, write_changes_to_working_tree};

use crate::core::archived_buf::ArchivedBuf;
use crate::core::rkyv::{Archive, Deserialize, Infallible, Serialize};
use crate::chunk::CompressedChunk;
use crate::clipmap::Level;
use crate::units::*;
use crate::vox::convert_vox_model_to_chunks;

use itertools::Itertools;
use sled::transaction::{abort, TransactionError};
use sled::{IVec, Transactional, Tree};
use std::collections::BTreeSet;

use self::meta_tree::MapDbMetadata;

type ArchivedIVec<T> = ArchivedBuf<T, IVec>;

#[derive(
    Archive, Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, PartialOrd, Ord, Serialize,
)]
#[archive(crate = "crate::core::rkyv")]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbortReason {
    /// Failed to find a path from the one parent version to another.
    NoPathExists,
    /// Failed to find a path from a version node to the root ancestor. (Missing link).
    NoPathExistsToRoot,
    /// Tried to reference [`VersionChanges`] that don't exist in the change tree.
    MissingVersionChanges,
}

/// # Map Database
///
/// This database is effectively the backing store for a [`ChunkClipMap`](crate::ChunkClipMap). It supports CRUD operations on
/// [`CompressedChunk`]s as well as a versioned log of changes.
///
/// ## Implementation
///
/// All user data is stored in three [`sled::Tree`]s.
///
/// ### Working Tree
///
/// One tree is used for the *working* [`Version`] of the map, and it stores all of the [`CompressedChunk`] data for the working
/// version. All new changes are written to this tree.
///
/// ### Backup Tree
///
/// As new changes are written, the old values are moved into the "backup tree." The backup tree is just a persistent buffer
/// that eventually gets archived when the working version is committed.
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

    // We keep the change tree and graph trees separate so that finding a path between versions does not require reading all of
    // the changes associated with each version.
    version_change_tree: Tree,
    version_graph_tree: Tree,

    /// HACK: We only have this type to work around sled's lack of transactional iteration. When archiving a version, we iterate
    /// over this set of keys and put the entries into the archive.
    backup_key_cache: BackupKeyCache,
    // Zero-copy isn't super important for this tiny struct, so we just copy it for convenience.
    cached_meta: MapDbMetadata,
}

impl MapDb {
    /// Opens the database. On first open, a single working version will be created with no parent version.
    pub fn open(db: &sled::Db, map_name: &str) -> Result<Self, TransactionError<AbortReason>> {
        let (meta_tree, cached_meta) = open_meta_tree(map_name, db)?;
        let version_change_tree = open_version_change_tree(map_name, db)?;
        let version_graph_tree = open_version_graph_tree(map_name, db)?;
        let (backup_tree, backup_key_cache) = open_backup_tree(map_name, db)?;
        let working_tree = open_working_tree(map_name, db)?;

        Ok(Self {
            meta_tree,
            working_tree,
            backup_tree,
            version_change_tree,
            version_graph_tree,
            backup_key_cache,
            cached_meta,
        })
    }

    /// Writes all data from `model` into `target_lod` of the working version.
    pub fn import_vox(&mut self, target_lod: Level, model: &vox_format::types::Model) -> Result<(), TransactionError> {
        let chunks = convert_vox_model_to_chunks(model);
        // Write the chunks into the database.
        let mut encoder = ChangeEncoder::default();
        for (ChunkUnits(chunk_coords), chunk) in chunks.into_iter() {
            encoder.add_compressed_change(ChunkDbKey::new(target_lod, chunk_coords.into()), Change::Insert(chunk.compress()));
        }
        self.write_working_version(encoder.encode())
    }

    pub fn cached_meta(&self) -> &MapDbMetadata {
        &self.cached_meta
    }

    /// Writes `changes` to the working version and stores the old values in the backup tree.
    pub fn write_working_version(
        &mut self,
        changes: EncodedChanges<CompressedChunk>,
    ) -> Result<(), TransactionError> {
        log::trace!("Writing to {:?}", self.cached_meta.working_version);
        let Self {
            working_tree,
            backup_tree,
            backup_key_cache,
            ..
        } = self;
        let new_backup_keys: Vec<_> =
            (&*working_tree, &*backup_tree).transaction(|(working_txn, backup_txn)| {
                let reverse_changes =
                    write_changes_to_working_tree(working_txn, backup_key_cache, changes.clone())?;
                let new_backup_keys = reverse_changes
                    .changes
                    .iter()
                    .map(|(key, _)| ChunkDbKey::from_sled_key(key))
                    .collect();
                write_changes_to_backup_tree(backup_txn, reverse_changes)?;
                Ok(new_backup_keys)
            })?;
        // Transaction succeeded, so add the new keys to the backup cache.
        for key in new_backup_keys.into_iter() {
            debug_assert!(!backup_key_cache.keys.contains(&key));
            backup_key_cache.keys.insert(key);
        }
        Ok(())
    }

    /// Reads the compressed bytes of the chunk at `key` for the working version.
    pub fn read_working_version(
        &self,
        key: ChunkDbKey,
    ) -> Result<Option<ArchivedChangeIVec<CompressedChunk>>, sled::Error> {
        let bytes = self.working_tree.get(IVec::from(&key.into_sled_key()))?;
        Ok(bytes.map(|b| unsafe { ArchivedIVec::<Change<CompressedChunk>>::new(b) }))
    }

    /// Archives the backup tree entries into a [`VersionChanges`] that gets serialized and stored in the version change tree
    /// with the current working [`Version`]. A new working version is generated and the old working version becomes the parent
    /// version.
    ///
    /// Nothing happens if the working version has no changes.
    pub fn commit_working_version(&mut self) -> Result<(), TransactionError<AbortReason>> {
        if self.backup_key_cache.keys.is_empty() {
            return Ok(());
        }

        log::trace!(
            "Committing non-empty {:?}",
            self.cached_meta.working_version
        );

        let new_meta = (
            &self.backup_tree,
            &self.version_graph_tree,
            &self.version_change_tree,
            &self.meta_tree,
        )
            .transaction(|(backup_txn, graph_txn, changes_txn, meta_txn)| {
                if let Some(parent) = self.cached_meta.parent_version {
                    log::trace!("Archiving {:?} from backup", parent);
                    archive_version(
                        changes_txn,
                        parent,
                        &commit_backup(backup_txn, &self.backup_key_cache)?,
                    )?;
                } else {
                    // We only need to do this once, but it's important for correctness.
                    clear_backup(backup_txn, &self.backup_key_cache)?;
                }
                link_version(
                    graph_txn,
                    self.cached_meta.working_version,
                    VersionNode {
                        parent_version: self.cached_meta.parent_version,
                    },
                )?;
                let new_meta = MapDbMetadata {
                    grandparent_version: self.cached_meta.parent_version,
                    parent_version: Some(self.cached_meta.working_version),
                    working_version: Version::new(graph_txn.generate_id()?),
                };
                write_meta(meta_txn, &new_meta)?;
                Ok(new_meta)
            })?;
        self.backup_key_cache.keys.clear();
        self.cached_meta = new_meta;
        Ok(())
    }

    /// Sets the parent version to `new_parent_version` and generates a new (empty) working child version.
    ///
    /// This will always `commit_working_version` before migrating to a new parent. If there is no parent for the current
    /// working version, then nothing happens.
    pub fn branch_from_version(
        &mut self,
        new_parent_version: Version,
    ) -> Result<(), TransactionError<AbortReason>> {
        // After committing, we may end up with a new empty working version. But it's not linked into the graph yet. We can just
        // abandon it, since it is empty.
        self.commit_working_version()?;

        let old_meta = self.cached_meta;

        if let Some(old_parent_version) = old_meta.parent_version {
            let new_meta = (
                &self.meta_tree,
                &self.version_graph_tree,
                &self.version_change_tree,
                &self.working_tree,
            )
                .transaction(|(meta_txn, graph_txn, change_txn, working_txn)| {
                    // Apply the archived changes from all versions between the old parent version and the new parent version,
                    // leaving behind the inverse changes.
                    let path = find_path_between_versions(
                        graph_txn,
                        old_parent_version,
                        new_parent_version,
                    )?;
                    let empty_backup_keys = BackupKeyCache {
                        keys: BTreeSet::default(),
                    };
                    log::trace!(
                        "Migrating from parent {:?} to parent {:?}",
                        old_parent_version,
                        new_parent_version
                    );
                    for (&prev_version, &next_version) in path.path.iter().tuple_windows() {
                        if let Some(changes) = remove_archived_version(change_txn, next_version)? {
                            let mut encoder = ChangeEncoder::default();
                            for (key, change) in changes.as_ref().changes.iter() {
                                let key: ChunkDbKey = key.deserialize(&mut Infallible).unwrap();
                                // PERF: in principle we should be able to copy the compressed bytes directly from the archived
                                // change, but the types aren't set up for that yet
                                let change = change.deserialize(&mut Infallible).unwrap();
                                encoder.add_compressed_change(key, change);
                            }
                            let reverse_changes = write_changes_to_working_tree(
                                working_txn,
                                &empty_backup_keys,
                                encoder.encode(),
                            )?;
                            let prev_version_changes = VersionChanges::from(&reverse_changes);
                            log::trace!("Archiving {:?} from working tree", prev_version,);
                            archive_version(change_txn, prev_version, &prev_version_changes)?;
                        } else {
                            return abort(AbortReason::MissingVersionChanges);
                        }
                    }
                    let new_working_version = Version::new(graph_txn.generate_id()?);
                    let new_meta = MapDbMetadata {
                        grandparent_version: path.end_parent,
                        parent_version: Some(new_parent_version),
                        working_version: new_working_version,
                    };
                    write_meta(meta_txn, &new_meta)?;
                    Ok(new_meta)
                })?;
            self.cached_meta = new_meta;
        }

        Ok(())
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
    use crate::chunk::Chunk;
    use crate::core::glam::IVec3;

    #[test]
    fn write_and_read_changes_same_version() {
        let db = sled::Config::default().temporary(true).open().unwrap();
        let mut map = MapDb::open(&db, "mymap").unwrap();

        let chunk_key = ChunkDbKey::new(1, IVec3::ZERO.into());
        let mut encoder = ChangeEncoder::default();
        encoder.add_compressed_change(chunk_key, Change::Insert(Chunk::default().compress()));
        map.write_working_version(encoder.encode()).unwrap();

        let chunk_compressed_bytes = map.read_working_version(chunk_key).unwrap().unwrap();
        assert_eq!(
            chunk_compressed_bytes.deserialize(),
            Change::Insert(Chunk::default().compress())
        );
    }

    #[test]
    fn commit_empty_working_version_does_nothing() {
        let db = sled::Config::default().temporary(true).open().unwrap();
        let mut map = MapDb::open(&db, "mymap").unwrap();

        assert_eq!(
            map.cached_meta(),
            &MapDbMetadata {
                grandparent_version: None,
                parent_version: None,
                working_version: Version::new(0),
            }
        );

        map.commit_working_version().unwrap();

        assert_eq!(
            map.cached_meta(),
            &MapDbMetadata {
                grandparent_version: None,
                parent_version: None,
                working_version: Version::new(0),
            }
        );
    }

    #[test]
    fn commit_multiple_versions_with_changes_and_branch() {
        let db = sled::Config::default().temporary(true).open().unwrap();
        let mut map = MapDb::open(&db, "mymap").unwrap();

        let chunk_key1 = ChunkDbKey::new(1, IVec3::ZERO.into());
        let mut encoder = ChangeEncoder::default();
        encoder.add_compressed_change(chunk_key1, Change::Insert(Chunk::default().compress()));
        map.write_working_version(encoder.encode()).unwrap();

        let v0 = map.cached_meta().working_version;
        map.commit_working_version().unwrap();

        // Undo the previous change.
        let mut encoder = ChangeEncoder::default();
        encoder.add_compressed_change(chunk_key1, Change::Remove);
        map.write_working_version(encoder.encode()).unwrap();

        let v1 = map.cached_meta().working_version;
        map.commit_working_version().unwrap();

        assert_eq!(
            map.cached_meta(),
            &MapDbMetadata {
                working_version: Version::new(2),
                parent_version: Some(v1),
                grandparent_version: Some(v0),
            }
        );

        // We removed the entry in this version.
        assert_eq!(map.read_working_version(chunk_key1).unwrap(), None);

        // But we can bring it back by reverting to v0.
        map.branch_from_version(v0).unwrap();

        let expected_insert = Ok(Some(unsafe {
            ArchivedChangeIVec::new(IVec::from(
                Change::Insert(Chunk::default().compress())
                    .serialize()
                    .as_ref(),
            ))
        }));

        assert_eq!(map.read_working_version(chunk_key1), expected_insert);

        // Commit changes to the branch.
        let chunk_key2 = ChunkDbKey::new(2, IVec3::ZERO.into());
        let mut encoder = ChangeEncoder::default();
        encoder.add_compressed_change(chunk_key2, Change::Insert(Chunk::default().compress()));
        map.write_working_version(encoder.encode()).unwrap();
        let v2 = map.cached_meta().working_version;
        map.commit_working_version().unwrap();

        // Branch from a sibling version.
        map.branch_from_version(v1).unwrap();
        assert_eq!(map.read_working_version(chunk_key1), Ok(None));
        assert_eq!(map.read_working_version(chunk_key2).unwrap(), None);

        // And back.
        map.branch_from_version(v2).unwrap();
        assert_eq!(map.read_working_version(chunk_key1), expected_insert);
        assert_eq!(map.read_working_version(chunk_key2), expected_insert);
    }
}
