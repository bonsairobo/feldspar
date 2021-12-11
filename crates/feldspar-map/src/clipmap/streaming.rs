use super::ChunkClipMap;
use crate::{Level, NodeLocation, Sphere};
use crate::glam::Vec3A;

use float_ord::FloatOrd;
use smallvec::SmallVec;

#[derive(Clone, Copy, Debug)]
pub struct StreamingConfig {
    /// A chunk is a *render candidate* if
    ///
    /// ```text
    /// D < R + clip_sphere.radius && (D / R) > detail
    /// ```
    ///
    /// where:
    ///
    ///   - `D` is the Euclidean distance from observer to the center of the chunk (in LOD0 space)
    ///   - `R` is the radius of the chunk's bounding sphere (in LOD0 space)
    pub detail: f32,
    /// The [`Level`] where we detect new nodes and insert loading ancestor nodes.
    pub load_level: Level,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            detail: 6.0,
            load_level: 4,
        }
    }
}

impl ChunkClipMap {
    pub fn new_nodes_intersecting_sphere(
        &self,
        config: StreamingConfig,
        old_clip_sphere: Sphere,
        new_clip_sphere: Sphere,
        mut rx: impl FnMut(NodeLocation),
    ) {
        // Note: exclude loading trees
        todo!()
    }

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
    pub new_chunks: SmallVec<[NodeLocation; 8]>,
}

/// Merge many `old_chunks` into `new_chunk`. The number of old chunks depends on how many levels of detail the octant has
/// moved.
#[derive(Clone, Debug, PartialEq)]
pub struct MergeChunks {
    pub old_chunks: SmallVec<[NodeLocation; 8]>,
    pub new_chunk: NodeLocation,
}

#[derive(Clone, Copy)]
struct ClosestNodeHeapElem {
    location: NodeLocation,
    bounding_sphere: Sphere,
    closest_dist_to_observer: f32,
}

impl ClosestNodeHeapElem {
    fn center_dist_to_observer(&self) -> f32 {
        self.closest_dist_to_observer + self.bounding_sphere.radius
    }
}

impl PartialEq for ClosestNodeHeapElem {
    fn eq(&self, other: &Self) -> bool {
        self.location.ptr == other.location.ptr
    }
}
impl Eq for ClosestNodeHeapElem {}

impl PartialOrd for ClosestNodeHeapElem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        FloatOrd(self.closest_dist_to_observer)
            .partial_cmp(&FloatOrd(other.closest_dist_to_observer))
            .map(|o| o.reverse())
    }
}

impl Ord for ClosestNodeHeapElem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        FloatOrd(self.closest_dist_to_observer).cmp(&FloatOrd(other.closest_dist_to_observer)).reverse()
    }
}
