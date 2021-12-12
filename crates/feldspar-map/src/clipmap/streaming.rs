use super::ChunkClipMap;
use crate::glam::{IVec3, Vec3A};
use crate::{
    chunk_bounding_sphere, sphere_intersecting_ancestor_chunk_extent, visit_children, ChunkUnits,
    Level, NodeLocation, Sphere, VoxelUnits,
};

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

pub struct NodeSlot {
    pub coordinates: ChunkUnits<IVec3>,
    pub level: Level,
    pub dist: f32,
    pub is_render_candidate: bool,
}

impl ChunkClipMap {
    /// Searches for all of the nodes marked as "loading." It is up to the caller to subsequently write or delete the data in
    /// the loading node so that it gets marked as "loaded".
    pub fn loading_nodes(
        &self,
        budget: usize,
        observer: VoxelUnits<Vec3A>,
        mut rx: impl FnMut(NodeSlot),
    ) {
        todo!()
    }

    /// Searches for nodes whose render detail should change.
    pub fn render_lod_changes(
        &self,
        budget: usize,
        observer: VoxelUnits<Vec3A>,
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
    bounding_sphere: VoxelUnits<Sphere>,
    closest_dist_to_observer: VoxelUnits<f32>,
}

impl ClosestNodeHeapElem {
    fn center_dist_to_observer(&self) -> VoxelUnits<f32> {
        VoxelUnits::map2(
            self.closest_dist_to_observer,
            self.bounding_sphere,
            |d, s| d + s.radius,
        )
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
        VoxelUnits::map2(
            self.closest_dist_to_observer,
            other.closest_dist_to_observer,
            |d1, d2| FloatOrd(d1).partial_cmp(&FloatOrd(d2)),
        )
        .into_inner()
        .map(|o| o.reverse())
    }
}

impl Ord for ClosestNodeHeapElem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        VoxelUnits::map2(
            self.closest_dist_to_observer,
            other.closest_dist_to_observer,
            |d1, d2| FloatOrd(d1).cmp(&FloatOrd(d2)),
        )
        .into_inner()
        .reverse()
    }
}

pub fn new_nodes_intersecting_sphere(
    config: StreamingConfig,
    root_level: Level,
    old_clip_sphere: VoxelUnits<Sphere>,
    new_clip_sphere: VoxelUnits<Sphere>,
    mut rx: impl FnMut(NodeSlot),
) {
    let ChunkUnits(new_root_level_extent) =
        sphere_intersecting_ancestor_chunk_extent(new_clip_sphere, root_level);

    for root_coords in new_root_level_extent.iter3() {
        new_nodes_intersecting_sphere_recursive(
            ChunkUnits(root_coords),
            root_level,
            config,
            old_clip_sphere,
            new_clip_sphere,
            &mut rx,
        );
    }
}

fn new_nodes_intersecting_sphere_recursive(
    node_coords: ChunkUnits<IVec3>,
    node_level: Level,
    config: StreamingConfig,
    old_clip_sphere: VoxelUnits<Sphere>,
    new_clip_sphere: VoxelUnits<Sphere>,
    rx: &mut impl FnMut(NodeSlot),
) {
    let VoxelUnits(old_clip_sphere) = old_clip_sphere;
    let VoxelUnits(new_clip_sphere) = new_clip_sphere;
    let VoxelUnits(node_sphere) = chunk_bounding_sphere(0, node_coords);

    let dist_to_new_clip_sphere = node_sphere.center.distance(new_clip_sphere.center);
    let node_intersects_new_clip_sphere =
        dist_to_new_clip_sphere - node_sphere.radius < new_clip_sphere.radius;

    if !node_intersects_new_clip_sphere {
        // There are no events for this node or any of its descendants.
        return;
    }

    if node_level > config.load_level {
        let child_level = node_level - 1;
        visit_children(node_coords.into_inner(), |_child_index, child_coords| {
            new_nodes_intersecting_sphere_recursive(
                ChunkUnits(child_coords),
                child_level,
                config,
                VoxelUnits(old_clip_sphere),
                VoxelUnits(new_clip_sphere),
                rx,
            );
        });
    } else {
        let dist_to_old_clip_sphere = node_sphere.center.distance(old_clip_sphere.center);
        let node_intersects_old_clip_sphere =
            dist_to_old_clip_sphere - node_sphere.radius < old_clip_sphere.radius;
        if !node_intersects_old_clip_sphere {
            // This is the LOD where we want to detect entrances into the clip sphere.
            let is_render_candidate =
                node_level == 0 || dist_to_new_clip_sphere / node_sphere.radius > config.detail;

            rx(NodeSlot {
                coordinates: node_coords,
                level: node_level,
                dist: dist_to_new_clip_sphere,
                is_render_candidate,
            });
        }
    }
}
