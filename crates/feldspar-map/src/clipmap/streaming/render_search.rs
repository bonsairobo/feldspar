use crate::clipmap::neighborhood_subdiv::{NEIGHBORHOODS, NEIGHBORHOODS_PARENTS};
use crate::clipmap::ChunkClipMap;
use crate::core::geometry::Sphere;
use crate::core::glam::{IVec3, Vec3A};
use crate::{
    clipmap::{ChildIndex, Level, NodeLocation, StateBit, VisitCommand},
    coordinates::{chunk_bounding_sphere, CUBE_CORNERS},
    units::*,
};

use float_ord::FloatOrd;
use grid_tree::{AllocPtr, NodeKey, NodePtr};
use smallvec::SmallVec;
use std::collections::BinaryHeap;

/// A chunk's desired sample rate has changed based on proximity to the center of the clip sphere.
#[derive(Clone, Debug, PartialEq)]
pub enum LodChange {
    /// The desired sample rate for this chunk decreased this frame.
    Merge(MergeChunks),
    /// The desired sample rate for this chunk increased this frame.
    ///
    /// `Box`ed because `SplitChunk` is a relatively large variant, as pointed out by clippy.
    /// PERF: measure the effects of this choice.
    Split(Box<SplitChunk>),
    /// This is just a `Merge` with no descendants.
    Spawn(RenderNeighborhood),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderNeighborhood {
    pub level: Level,
    pub neighbors: [Neighbor; 8],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Neighbor {
    /// This node exists in the clipmap octree. It might be loading, but we need to check the
    /// [`NodeState`](crate::clipmap::NodeState).
    Occupied(AllocPtr),
    /// An ancestor indicated this slot was empty.
    Empty { loaded: bool },
}

impl Neighbor {
    fn unwrap_occupied(self) -> AllocPtr {
        match self {
            Self::Occupied(ptr) => ptr,
            _ => panic!("Tried to unwrap on Neighbor::Empty"),
        }
    }
}

/// Split `old_chunk` into children `new_chunks`.
#[derive(Clone, Debug, PartialEq)]
pub struct SplitChunk {
    pub old_chunk: NodeLocation,
    pub new_chunks: [Option<RenderNeighborhood>; 8],
}

/// Merge many `old_chunks` into `new_chunk`. The number of old chunks depends on how many levels of detail the octant has
/// moved.
#[derive(Clone, Debug, PartialEq)]
pub struct MergeChunks {
    pub old_chunks: SmallVec<[NodeLocation; 8]>,
    pub new_chunk: RenderNeighborhood,
}

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

        // Put root neighborhoods in the candidate heap.
        for root_key in self.octree.iter_root_keys() {
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

        // Recursively search for changes in render LOD.
        //
        // This is finding the cross section of nodes in the octree that are active for rendering, then diffing it with the
        // previously active ancestor (split) or descendants (merge). By "cross section," we mean that along any path from root
        // node to leaf node, there is exactly one active node.
        let mut num_render_chunks = 0;
        while let Some(RenderSearchNode {
            level,
            coordinates: ChunkUnits(coordinates),
            neighborhood,
            center_dist_to_observer,
            bounding_sphere_radius,
            ..
        }) = candidate_heap.pop()
        {
            // It's OK to go a little over budget for the sake of simple atomicity.
            if num_render_chunks >= budget {
                break;
            }

            let nhood = RenderNeighborhood {
                level,
                neighbors: neighborhood,
            };

            // Min neighbor in the candidate heap must always be occupied.
            let min_neighbor_ptr = NodePtr::new(level, nhood.neighbors[0].unwrap_occupied());
            let min_node = self.octree.get_value(min_neighbor_ptr).unwrap();
            let min_node_state = min_node.state();
            let min_is_loading = min_node_state.is_loading();
            let was_active = min_node_state.is_rendering();

            // Determine whether this node is "active" based on the StreamingConfig::detail threshold.
            let VoxelUnits(dist_to_observer) = center_dist_to_observer;
            let VoxelUnits(node_radius) = bounding_sphere_radius;
            let VoxelUnits(detail) = self.stream_config.detail;
            // NB: is_active = false implies we are not at level 0.
            let is_active = level == 0 || dist_to_observer / node_radius > detail;

            match (was_active, is_active) {
                // Old and new agree this node is active. No need to merge or split. None of the descendants can merge or split
                // either.
                (true, true) => (),
                // Old and new frames agree this node is not active. Keep searching down this path if possible.
                (false, false) => {
                    // Add all child neighborhoods to the heap.
                    let child_neighborhoods =
                        self.construct_child_neighborhoods(min_neighbor_ptr, &nhood.neighbors);
                    for (&child_offset, n) in
                        CUBE_CORNERS.iter().zip(child_neighborhoods.into_iter())
                    {
                        if let Some(child_neighborhood) = n {
                            candidate_heap.push(RenderSearchNode::new(
                                child_neighborhood.level,
                                ChunkUnits(coordinates + child_offset),
                                child_neighborhood.neighbors,
                                VoxelUnits(observer),
                            ));
                        }
                    }
                }
                // This node just became inactive, and none of its ancestors were active, so it must have active descendants.
                (true, false) => {
                    // This chunk could potentially need to split over multiple levels, but we need to be careful not to run out
                    // of render chunk budget. To be fair to other chunks in the queue that need to be split, we will only split
                    // by one level for now.

                    let child_neighborhoods =
                        self.construct_child_neighborhoods(min_neighbor_ptr, &nhood.neighbors);

                    // Make sure all child neighborhoods are loaded.
                    for nhood in child_neighborhoods.iter().flatten() {
                        if !self.neighborhood_is_loaded(nhood) {
                            continue;
                        }
                    }

                    // At this point, we've committed to meshing the children.
                    min_node_state.state.unset_bit(StateBit::Render as u8);
                    self.octree
                        .visit_children(min_neighbor_ptr, |child_ptr, _| {
                            let child_node = self.octree.get_value(child_ptr).unwrap();
                            child_node.state().state.set_bit(StateBit::Render as u8);
                            num_render_chunks += 1;
                        });
                    rx(LodChange::Split(Box::new(SplitChunk {
                        old_chunk: NodeLocation::new(ChunkUnits(coordinates), min_neighbor_ptr),
                        new_chunks: child_neighborhoods,
                    })));
                }
                // Node just became active, and none of its ancestors were active.
                (false, true) => {
                    // Make sure the neighborhood is loaded.
                    if min_is_loading || !self.neighborhood_is_loaded(&nhood) {
                        continue;
                    }

                    // At this point we've committed to meshing this node.
                    min_node_state.state.set_bit(StateBit::Render as u8);
                    num_render_chunks += 1;

                    if level == 0 {
                        // No descendants to merge.
                        rx(LodChange::Spawn(nhood));
                        continue;
                    }

                    // This node might have active descendants. Merge those active descendants into this node.
                    let mut deactivate_nodes = SmallVec::<[NodeLocation; 8]>::new();
                    self.octree
                        .visit_children(min_neighbor_ptr, |child_ptr, _| {
                            self.octree.visit_tree_depth_first(
                                child_ptr,
                                coordinates,
                                0,
                                |node_ptr, node_coords| {
                                    let descendant_node = self.octree.get_value(node_ptr).unwrap();
                                    let descendant_was_active = descendant_node
                                        .state()
                                        .state
                                        .fetch_and_unset_bit(StateBit::Render as u8);
                                    if descendant_was_active {
                                        deactivate_nodes.push(NodeLocation::new(
                                            ChunkUnits(node_coords),
                                            node_ptr,
                                        ));
                                        VisitCommand::SkipDescendants
                                    } else {
                                        VisitCommand::Continue
                                    }
                                },
                            )
                        });

                    if deactivate_nodes.is_empty() {
                        rx(LodChange::Spawn(nhood));
                    } else {
                        rx(LodChange::Merge(MergeChunks {
                            old_chunks: deactivate_nodes,
                            new_chunk: nhood,
                        }));
                    }
                }
            }
        }
    }

    fn construct_child_neighborhoods(
        &self,
        min_neighbor_ptr: NodePtr,
        neighborhood: &[Neighbor; 8],
    ) -> [Option<RenderNeighborhood>; 8] {
        debug_assert!(min_neighbor_ptr.level() > 0);
        let parent_level = min_neighbor_ptr.level();

        let mut child_neighborhoods = [None; 8];

        // We will create a 2^3 neighborhood with each of these children as the minimum.
        let min_children = self.octree.child_pointers(min_neighbor_ptr).unwrap();

        // Add all child neighborhoods to the heap.
        let child_level = min_neighbor_ptr.level() - 1;
        for (child_index, (parent_indices, child_indices)) in NEIGHBORHOODS_PARENTS
            .iter()
            .zip(NEIGHBORHOODS.iter())
            .enumerate()
        {
            // Only non-minimal neighbors can be empty when meshing.
            if min_children.get_child(child_index as ChildIndex).is_none() {
                continue;
            }

            // Fill out each node in this neighborhood.
            let mut child_neighborhood = [Neighbor::Empty { loaded: false }; 8];
            for (target_neighbor, (&parent_i, &child_i)) in child_neighborhood
                .iter_mut()
                .zip(parent_indices.iter().zip(child_indices.iter()))
            {
                // PERF: Lame that we will match on the same parent multiple times? Would probably need to invert the lookup
                // tables to avoid that.
                let parent = &neighborhood[parent_i as usize];
                *target_neighbor = match *parent {
                    Neighbor::Occupied(parent_ptr) => {
                        let parent_ptr = NodePtr::new(parent_level, parent_ptr);
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
                    empty => empty,
                };
            }

            child_neighborhoods[child_index] = Some(RenderNeighborhood {
                level: child_level,
                neighbors: child_neighborhood,
            });
        }

        child_neighborhoods
    }

    fn neighborhood_is_loaded(&self, nhood: &RenderNeighborhood) -> bool {
        // PERF: This does redundant checks of the same node.
        for neighbor in nhood.neighbors {
            match neighbor {
                Neighbor::Occupied(ptr) => {
                    let ptr = NodePtr::new(nhood.level, ptr);
                    let node = self.octree.get_value(ptr).unwrap();
                    if node.state().is_loading() {
                        return false;
                    }
                }
                Neighbor::Empty { loaded } => {
                    if !loaded {
                        return false;
                    }
                }
            }
        }
        true
    }
}

#[derive(Clone)]
struct RenderSearchNode {
    level: Level,
    coordinates: ChunkUnits<IVec3>,
    closest_dist_to_observer: VoxelUnits<f32>,
    center_dist_to_observer: VoxelUnits<f32>,
    bounding_sphere_radius: VoxelUnits<f32>,
    neighborhood: [Neighbor; 8],
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
            bounding_sphere_radius: VoxelUnits(bounding_sphere.radius),
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
