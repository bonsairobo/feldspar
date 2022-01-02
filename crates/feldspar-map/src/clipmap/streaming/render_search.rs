use crate::clipmap::neighborhood_subdiv::{NEIGHBORHOODS, NEIGHBORHOODS_PARENTS};
use crate::clipmap::ChunkClipMap;
use crate::core::geometry::Sphere;
use crate::core::glam::{IVec3, Vec3A};
use crate::{
    clipmap::{Level, NodeLocation},
    coordinates::{
        child_index, chunk_bounding_sphere, sphere_intersecting_ancestor_chunk_extent,
        visit_children, CUBE_CORNERS,
    },
    units::*,
};

use float_ord::FloatOrd;
use grid_tree::{AllocPtr, ChildIndex, NodeKey, NodePtr};
use smallvec::SmallVec;
use std::collections::BinaryHeap;

impl ChunkClipMap {
    /// Searches for up to `budget` nodes whose render detail should change.
    ///
    /// This only includes nodes whose entire "chunk neighborhood" is loaded, since we need to reference voxel neighborhoods to
    /// generate correct meshes.
    pub fn render_lod_changes(
        &self,
        budget: usize,
        observer: VoxelUnits<Vec3A>,
        mut rx: impl FnMut(LodChange),
    ) {
        let mut candidate_heap = BinaryHeap::new();
        let mut num_load_slots = 0;

        for (root_key, root_node) in self.octree.iter_roots() {
            let mut neighborhood = [Neighbor::new(Some(root_node.self_ptr)); 8];
            for (&offset, neighbor) in CUBE_CORNERS.iter().zip(neighborhood.iter_mut()).skip(1) {
                let neighbor_key = NodeKey::new(root_key.level, root_key.coordinates + offset);
                *neighbor = Neighbor::new(
                    self.octree
                        .find_root(neighbor_key)
                        .map(|node| node.self_ptr),
                );
            }
            candidate_heap.push(RenderSearchNode::new(
                root_key.level,
                ChunkUnits(root_key.coordinates),
                neighborhood,
                observer,
            ));
        }

        while let Some(RenderSearchNode {
            level,
            coordinates,
            neighborhood,
            ..
        }) = candidate_heap.pop()
        {
            if num_load_slots >= budget {
                break;
            }

            if level == 0 {
                continue;
            }
            let child_level = level - 1;

            todo!();
        }
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

#[derive(Clone)]
struct RenderSearchNode {
    level: Level,
    coordinates: ChunkUnits<IVec3>,
    closest_dist_to_observer: VoxelUnits<f32>,
    neighborhood: [Neighbor; 8],
}

#[derive(Clone, Copy)]
struct Neighbor {
    ptr: Option<AllocPtr>,
}

impl Neighbor {
    fn new(ptr: Option<AllocPtr>) -> Self {
        Self { ptr }
    }
}

impl RenderSearchNode {
    fn new(
        level: Level,
        coordinates: ChunkUnits<IVec3>,
        neighborhood: [Neighbor; 8],
        observer: VoxelUnits<Vec3A>,
    ) -> Self {
        let VoxelUnits(observer) = observer;
        let VoxelUnits(bounding_sphere) = chunk_bounding_sphere(level, coordinates);

        let center_dist_to_observer = observer.distance(bounding_sphere.center);
        // Subtract the bounding sphere's radius to estimate the distance from the observer to the *closest point* on the chunk.
        // This should make it more fair for higher LODs.
        let closest_dist_to_observer = center_dist_to_observer - bounding_sphere.radius;

        Self {
            level,
            coordinates,
            closest_dist_to_observer: VoxelUnits(closest_dist_to_observer),
            neighborhood,
        }
    }
}

impl PartialEq for RenderSearchNode {
    fn eq(&self, other: &Self) -> bool {
        self.level == other.level && self.coordinates == other.coordinates
    }
}
impl Eq for RenderSearchNode {}

impl PartialOrd for RenderSearchNode {
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

impl Ord for RenderSearchNode {
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
