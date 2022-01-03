use crate::clipmap::neighborhood_subdiv::{NEIGHBORHOODS, NEIGHBORHOODS_PARENTS};
use crate::clipmap::ChunkClipMap;
use crate::core::geometry::Sphere;
use crate::core::glam::{IVec3, Vec3A};
use crate::{
    clipmap::{Level, NodeLocation},
    coordinates::{chunk_bounding_sphere, CUBE_CORNERS},
    units::*,
};

use float_ord::FloatOrd;
use grid_tree::{AllocPtr, NodeKey, NodePtr};
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
        let VoxelUnits(observer) = observer;
        let VoxelUnits(clip_radius) = self.stream_config.clip_sphere_radius;
        let clip_sphere = Sphere::new(observer, clip_radius);

        let node_intersects_clip_sphere = |key: NodeKey<IVec3>| {
            let VoxelUnits(chunk_bounding_sphere) =
                chunk_bounding_sphere(key.level, ChunkUnits(key.coordinates));
            clip_sphere.intersects(&chunk_bounding_sphere)
        };

        let mut candidate_heap = BinaryHeap::new();

        for (root_key, _root_node) in self.octree.iter_roots() {
            let mut neighborhood = [Neighbor::Empty { loaded: false }; 8];
            for (&offset, target_neighbor) in CUBE_CORNERS.iter().zip(neighborhood.iter_mut()) {
                let neighbor_key = NodeKey::new(root_key.level, root_key.coordinates + offset);

                *target_neighbor = if let Some(root_node) = self.octree.find_root(neighbor_key) {
                    Neighbor::Occupied(root_node.self_ptr)
                } else {
                    Neighbor::Empty {
                        loaded: node_intersects_clip_sphere(neighbor_key),
                    }
                };
            }
            candidate_heap.push(RenderSearchNode::new(
                root_key.level,
                ChunkUnits(root_key.coordinates),
                neighborhood,
                VoxelUnits(observer),
            ));
        }

        let mut num_load_slots = 0;
        while let Some(RenderSearchNode {
            level,
            coordinates: ChunkUnits(coordinates),
            neighborhood,
            center_dist_to_observer,
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

            // Add all child neighborhoods to the heap.
            for (&child_offset, (parent_indices, child_indices)) in CUBE_CORNERS
                .iter()
                .zip(NEIGHBORHOODS_PARENTS.iter().zip(NEIGHBORHOODS.iter()))
            {
                // Fill out each node in this neighborhood.
                let mut child_neighborhood = [Neighbor::Empty { loaded: false }; 8];
                for (target_neighbor, (&parent_i, &child_i)) in child_neighborhood
                    .iter_mut()
                    .zip(parent_indices.iter().zip(child_indices.iter()))
                {
                    // PERF: Lame that we will match on the same parent multiple times? Would probably need to invert the lookup
                    // tables to avoid that.
                    let parent = &neighborhood[parent_i as usize];
                    *target_neighbor = match parent {
                        &Neighbor::Occupied(parent_ptr) => {
                            let parent_ptr = NodePtr::new(level, parent_ptr);
                            let children = self.octree.child_pointers(parent_ptr).unwrap();
                            if let Some(child_ptr) = children.get_child(child_i) {
                                Neighbor::Occupied(child_ptr.alloc_ptr())
                            } else {
                                let parent_node = self.octree.get_value(parent_ptr).unwrap();
                                let loaded = parent_node
                                    .state()
                                    .descendant_is_loading
                                    .bit_is_set(child_i);
                                Neighbor::Empty { loaded }
                            }
                        }
                        &empty => empty,
                    };
                }

                // PERF: Should we skip empty minimal neighbors earlier?
                // Only non-minimal neighbors can be empty when meshing.
                if let Neighbor::Occupied(_) = child_neighborhood[0] {
                    candidate_heap.push(RenderSearchNode::new(
                        child_level,
                        ChunkUnits(coordinates + child_offset),
                        child_neighborhood,
                        VoxelUnits(observer),
                    ));
                }
            }
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
    center_dist_to_observer: VoxelUnits<f32>,
    neighborhood: [Neighbor; 8],
}

#[derive(Clone, Copy)]
enum Neighbor {
    /// This node exists in the clipmap octree. It might be loading, but we need to check the
    /// [`NodeState`](crate::clipmap::NodeState).
    Occupied(AllocPtr),
    /// An ancestor indicated this slot was empty.
    Empty { loaded: bool },
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
            center_dist_to_observer: VoxelUnits(center_dist_to_observer),
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
