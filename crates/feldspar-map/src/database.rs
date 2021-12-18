mod backup_tree;
mod change_encoder;
mod chunk_key;
mod meta_tree;
mod version_change_tree;
mod version_graph_tree;
mod working_tree;

pub use change_encoder::*;
pub use chunk_key::ChunkDbKey;

use backup_tree::{
    clear_backup, commit_backup, open_backup_tree, write_changes_to_backup_tree, BackupKeyCache,
};
use meta_tree::{open_meta_tree, read_meta_or_abort, write_meta};
use version_change_tree::{
    archive_version, get_archived_version, open_version_change_tree, VersionChanges,
};
use version_graph_tree::{link_version, open_version_graph_tree};
use working_tree::{open_working_tree, write_changes_to_working_tree};

use crate::archived_buf::ArchivedBuf;
use crate::CompressedChunk;

use rkyv::{Archive, Deserialize, Serialize};
use sled::transaction::TransactionError;
use sled::{IVec, Transactional, Tree};

use self::meta_tree::MapDbMetadata;

type ArchivedIVec<T> = ArchivedBuf<T, IVec>;

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
/// One tree is used for the *working* [`Version`] of the map, and it stores all of the [`CompressedChunk`] data for the working
/// version. All new changes are written to this tree.
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
    pub fn open(db: &sled::Db, map_name: &str) -> Result<Self, TransactionError> {
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

    pub fn cached_meta(&self) -> &MapDbMetadata {
        &self.cached_meta
    }

    /// Writes `changes` to the working version and stores the old values in the backup tree.
    pub fn write_working_version(
        &mut self,
        changes: EncodedChanges<CompressedChunk>,
    ) -> Result<(), TransactionError> {
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
                    .map(|(key, _)| ChunkDbKey::from_be_bytes(key))
                    .collect();
                write_changes_to_backup_tree(backup_txn, reverse_changes)?;
                Ok(new_backup_keys)
            })?;
        // Transaction succeeded, so add the new keys to the backup cache.
        for key in new_backup_keys.into_iter() {
            backup_key_cache.keys.insert(key);
        }
        Ok(())
    }

    /// Reads the compressed bytes of the chunk at `key` for the working version.
    pub fn read_working_version(&self, key: ChunkDbKey) -> Result<Option<IVec>, sled::Error> {
        self.working_tree
            .get(IVec::from(&ChunkDbKey::to_be_bytes(&key)))
    }

    /// Archives the backup tree entries into a [`VersionChanges`] that gets serialized and stored in the version archive tree
    /// with the current working [`Version`]. A new working version is generated and the old working version becomes the parent
    /// version.
    pub fn commit_working_version(&mut self) -> Result<(), TransactionError<()>> {
        let new_meta = (
            &self.backup_tree,
            &self.version_graph_tree,
            &self.version_change_tree,
            &self.meta_tree,
        )
            .transaction(
                |(backup_txn, version_graph_txn, version_changes_txn, meta_txn)| {
                    let mut meta = read_meta_or_abort(meta_txn)?.deserialize();
                    if let Some(parent) = self.cached_meta.parent_version {
                        let backup_changes = commit_backup(backup_txn, &self.backup_key_cache)?;
                        link_version(version_graph_txn, parent, meta.grandparent_version)?;
                        let version_changes = VersionChanges::new(backup_changes);
                        archive_version(version_changes_txn, parent, &version_changes)?;
                    } else {
                        // We generally only need to do this once, but it's important for correctness.
                        clear_backup(backup_txn, &self.backup_key_cache)?;
                    }
                    meta.grandparent_version = meta.parent_version;
                    meta.parent_version = Some(meta.working_version);
                    meta.working_version = Version::new(version_graph_txn.generate_id()?);
                    write_meta(meta_txn, &meta)?;
                    Ok(meta)
                },
            )?;
        self.backup_key_cache.keys.clear();
        self.cached_meta = new_meta;
        Ok(())
    }

    /// Sets the parent version to `parent_version` and generates a new (empty) working child version.
    pub fn branch_from_version(&self, parent_version: Version) -> Result<(), TransactionError<()>> {
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

    #[test]
    fn write_and_read_changes_same_version() {
        let db = sled::Config::default().temporary(true).open().unwrap();
        let mut map = MapDb::open(&db, "mymap").unwrap();

        let chunk_key = ChunkDbKey::new(1, IVec3::ZERO.into());
        let mut encoder = ChangeEncoder::default();
        encoder.add_compressed_change(chunk_key, Change::Insert(Chunk::default().compress()));
        map.write_working_version(encoder.encode()).unwrap();

        let chunk_compressed_bytes = map.read_working_version(chunk_key).unwrap().unwrap();
        let chunk = Chunk::from_compressed_bytes(&chunk_compressed_bytes);
        assert_eq!(chunk, Chunk::default());
    }

    #[test]
    fn commit_working_version_generates_new_version_and_updates_metadata() {
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
                parent_version: Some(Version::new(0)),
                working_version: Version::new(1),
            }
        );

        map.commit_working_version().unwrap();

        assert_eq!(
            map.cached_meta(),
            &MapDbMetadata {
                grandparent_version: Some(Version::new(0)),
                parent_version: Some(Version::new(1)),
                working_version: Version::new(2),
            }
        );
    }

    #[test]
    fn commit_multiple_versions_with_changes_and_branch() {
        let db = sled::Config::default().temporary(true).open().unwrap();
        let mut map = MapDb::open(&db, "mymap").unwrap();

        let chunk_key = ChunkDbKey::new(1, IVec3::ZERO.into());
        let mut encoder = ChangeEncoder::default();
        encoder.add_compressed_change(chunk_key, Change::Insert(Chunk::default().compress()));
        map.write_working_version(encoder.encode()).unwrap();

        let v0 = map.cached_meta().working_version;
        map.commit_working_version().unwrap();

        // Undo the previous change.
        let mut encoder = ChangeEncoder::default();
        encoder.add_compressed_change(chunk_key, Change::Remove);
        map.write_working_version(encoder.encode()).unwrap();

        map.commit_working_version().unwrap();

        // We removed the entry in this version.
        assert_eq!(map.read_working_version(chunk_key).unwrap(), None);

        // But we can bring it back by reverting to v0.
        map.branch_from_version(v0).unwrap();

        let value_bytes = map.read_working_version(chunk_key).unwrap().unwrap();
        let chunk = Chunk::from_compressed_bytes(value_bytes.as_ref());
        assert_eq!(chunk, Chunk::default());
    }
}
