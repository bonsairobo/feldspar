use super::{AbortReason, ArchivedIVec, Version};
use crate::core::rkyv::{
    ser::{serializers::CoreSerializer, Serializer},
    AlignedBytes, Archive, Deserialize, Serialize,
};

use sled::{
    transaction::{
        abort, ConflictableTransactionError, TransactionalTree, UnabortableTransactionError,
    },
    Tree,
};

#[derive(Archive, Debug, Deserialize, Serialize)]
#[archive(crate = "crate::core::rkyv")]
pub struct VersionNode {
    /// The version immediately before this one.
    pub parent_version: Option<Version>,
}

impl VersionNode {
    pub fn serialize(&self) -> AlignedBytes<16> {
        let mut serializer = CoreSerializer::<16, 0>::default();
        serializer.serialize_value(self).unwrap();
        serializer.into_serializer().into_inner()
    }
}

pub fn open_version_graph_tree(map_name: &str, db: &sled::Db) -> sled::Result<Tree> {
    db.open_tree(format!("{}-version-graph", map_name))
}

pub fn link_version(
    txn: &TransactionalTree,
    version: Version,
    node: VersionNode,
) -> Result<(), UnabortableTransactionError> {
    let key_bytes = version.into_sled_key();
    txn.insert(&key_bytes, node.serialize().as_ref())?;
    Ok(())
}

pub struct VersionPath {
    /// The path from `start_version` to `end_version`, inclusive.
    pub path: Vec<Version>,
    /// The parent of the last version in `path`.
    pub end_parent: Option<Version>,
}

pub fn find_path_between_versions(
    txn: &TransactionalTree,
    start_version: Version,
    end_version: Version,
) -> Result<VersionPath, ConflictableTransactionError<AbortReason>> {
    // First we search through the ancestors of start_version until hitting the root.
    let (path_result, start_path) = find_ancestor_path(txn, start_version, end_version)?;
    if let PathResult::FoundEnd = path_result {
        return Ok(start_path);
    }

    // If we didn't see the end_version, then it's not an ancestor, so we need to find the nearest common ancestor.
    let start_root_version = *start_path.path.last().unwrap();
    let (_path_result, mut end_path) = find_ancestor_path(txn, end_version, start_root_version)?;
    let end_root_version = *end_path.path.last().unwrap();

    if start_root_version != end_root_version {
        // No path exists. Programmer error?
        return abort(AbortReason::NoPathExists);
    }

    // Compare paths to the root to find the nearest common ancestor.
    let mut start_join = 0;
    let mut finish_join = 0;
    for ((i1, v1), (i2, v2)) in start_path
        .path
        .iter()
        .enumerate()
        .rev()
        .zip(end_path.path.iter().enumerate().rev())
    {
        if v1 != v2 {
            // The previous index held the nearest common ancestor.
            break;
        }
        start_join = i1;
        finish_join = i2;
    }

    let mut path = start_path.path[..=start_join].to_vec();
    let further_slice = &mut end_path.path[..finish_join];
    further_slice.reverse();
    path.extend_from_slice(further_slice);

    Ok(VersionPath {
        path,
        end_parent: end_path.end_parent,
    })
}

/// Finds a path along only ancestors, starting at `start_version` and ending at either `end_version` or the root ancestor,
/// whichever comes first.
pub fn find_ancestor_path(
    txn: &TransactionalTree,
    start_version: Version,
    end_version: Version,
) -> Result<(PathResult, VersionPath), ConflictableTransactionError<AbortReason>> {
    let mut path = vec![start_version];

    // First we search through the ancestors of start_version until hitting the root.
    let mut current_version = start_version;
    while let Some(node_bytes) = txn.get(current_version.into_sled_key())? {
        let node = unsafe { ArchivedIVec::<VersionNode>::new(node_bytes) }.deserialize();
        if current_version == end_version {
            return Ok((
                PathResult::FoundEnd,
                VersionPath {
                    path,
                    end_parent: node.parent_version,
                },
            ));
        }

        if let Some(parent) = node.parent_version {
            path.push(parent);
            current_version = parent;
        } else {
            // This must be the root version.
            return Ok((
                PathResult::FoundRoot,
                VersionPath {
                    path,
                    end_parent: None,
                },
            ));
        }
    }

    // We expect all nodes to have a path to the root.
    abort(AbortReason::NoPathExistsToRoot)
}

pub enum PathResult {
    FoundRoot,
    FoundEnd,
}
