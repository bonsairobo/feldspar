use super::ChunkClipMap;
use crate::NodeLocation;
use crate::glam::Vec3A;

impl ChunkClipMap {
    /// Searches for all of the nodes marked as "loading." It is up to the caller to subsequently write or delete the data in
    /// the loading node so that it gets marked as "loaded".
    pub fn loading_nodes(
        &self,
        budget: usize,
        observer: Vec3A,
        mut rx: impl FnMut(NodeLocation),
    ) {
        todo!()
    }

    /// Searches for nodes whose render detail should change.
    pub fn render_updates(
        &self,
        budget: usize,
        detail: f32,
        observer: Vec3A,
        mut rx: impl FnMut(LodChange),
    ) {
        todo!()
    }
}

/// A chunk's desired sample rate has changed based on proximity to the center of the clip sphere.
#[derive(Clone, Debug, PartialEq)]
pub enum LodChange {
    /// This is just a `Merge` with no descendants.
    Spawn(NodeLocation),
    /// The desired sample rate for this chunk increased this frame.
    Split(SplitChunk),
    /// The desired sample rate for this chunk decreased this frame.
    Merge(MergeChunks),
}

/// Split `old_chunk` into many `new_chunks`. The number of new chunks depends on how many levels of detail the octant has
/// moved.
#[derive(Clone, Debug, PartialEq)]
pub struct SplitChunk {
    pub old_chunk: NodeLocation,
    pub new_chunks: Vec<NodeLocation>,
}

/// Merge many `old_chunks` into `new_chunk`. The number of old chunks depends on how many levels of detail the octant has
/// moved.
#[derive(Clone, Debug, PartialEq)]
pub struct MergeChunks {
    pub old_chunks: Vec<NodeLocation>,
    pub new_chunk: NodeLocation,
}
