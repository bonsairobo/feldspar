use crate::clipmap::neighborhood_subdiv::{NEIGHBORHOODS, NEIGHBORHOODS_PARENTS};
use crate::clipmap::{ChunkClipMap, NodeState};
use crate::core::geometry::Sphere;
use crate::core::glam::{IVec3, Vec3A};
use crate::{
    clipmap::{ChildIndex, ChunkNode, Level, NodeLocation, StreamingConfig, VisitCommand},
    coordinates::{chunk_bounding_sphere, CUBE_CORNERS},
    units::*,
};

use float_ord::FloatOrd;
use grid_tree::{AllocPtr, NodeKey, NodePtr, OctreeI32};
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
    /// This node exists in the clipmap octree. It might be loading, but we need to check the [`NodeState`].
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
    pub fn render_search(&self, observer: VoxelUnits<Vec3A>, budget: usize) -> RenderSearch<'_> {
        RenderSearch::new(self.stream_config, &self.octree, observer, budget)
    }
}

pub struct RenderSearch<'a> {
    config: StreamingConfig,
    octree: &'a OctreeI32<ChunkNode>,
    clip_sphere: VoxelUnits<Sphere>,
    budget: usize,
    candidate_heap: BinaryHeap<RenderSearchNode>,
    num_render_chunks: usize,
}

impl<'a> Iterator for RenderSearch<'a> {
    type Item = LodChange;

    fn next(&mut self) -> Option<Self::Item> {
        let mut next_find = None;
        while next_find.is_none() && !self.is_done() {
            next_find = self.check_next_candidate();
        }
        next_find
    }
}

impl<'a> RenderSearch<'a> {
    fn new(
        config: StreamingConfig,
        octree: &'a OctreeI32<ChunkNode>,
        observer: VoxelUnits<Vec3A>,
        budget: usize,
    ) -> Self {
        let VoxelUnits(observer) = observer;
        let VoxelUnits(clip_radius) = config.clip_sphere_radius;
        let clip_sphere = VoxelUnits(Sphere::new(observer, clip_radius));
        let mut search = Self {
            config,
            octree,
            clip_sphere,
            budget,
            candidate_heap: BinaryHeap::new(),
            num_render_chunks: 0,
        };
        search.add_root_neighborhoods_to_heap();
        search
    }

    fn add_root_neighborhoods_to_heap(&mut self) {
        let VoxelUnits(clip_sphere) = &self.clip_sphere;

        // Put root neighborhoods in the candidate heap.
        for root_key in self.octree.iter_root_keys() {
            let mut neighborhood = [Neighbor::Empty { loaded: false }; 8];
            for (&offset, target_neighbor) in CUBE_CORNERS.iter().zip(neighborhood.iter_mut()) {
                let neighbor_key = NodeKey::new(root_key.level, root_key.coordinates + offset);
                if let Some(root_node) = self.octree.find_root(neighbor_key) {
                    *target_neighbor = Neighbor::Occupied(root_node.self_ptr)
                }
            }
            self.candidate_heap.push(RenderSearchNode::new(
                root_key.level,
                ChunkUnits(root_key.coordinates),
                neighborhood,
                VoxelUnits(clip_sphere.center),
            ));
        }
    }

    pub fn is_done(&self) -> bool {
        self.num_render_chunks > self.budget || self.candidate_heap.is_empty()
    }

    pub fn check_next_candidate(&mut self) -> Option<LodChange> {
        // Recursively search for changes in render LOD.
        //
        // This is finding the cross section of nodes in the octree that are active for rendering, then diffing it with the
        // previously active ancestor (split) or descendants (merge). By "cross section," we mean that along any path from root
        // node to leaf node, there is exactly one active node.

        self.candidate_heap.pop().and_then(|search_node| {
            let RenderSearchNode {
                level,
                coordinates: ChunkUnits(coordinates),
                neighborhood,
                center_dist_to_observer,
                bounding_sphere_radius,
                ..
            } = search_node;

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
            let VoxelUnits(detail) = self.config.detail;
            // NB: is_active = false implies we are not at level 0.
            let is_active = level == 0 || dist_to_observer / node_radius > detail;

            match (was_active, is_active) {
                // Old and new agree this node is active. No need to merge or split. None of the descendants can merge or split
                // either.
                (true, true) => None,
                // Old and new frames agree this node is not active. Keep searching down this path if possible.
                (false, false) => {
                    self.add_child_neighborhoods_to_heap(coordinates, &nhood, min_neighbor_ptr);
                    None
                }
                // This node just became inactive, and none of its ancestors were active, so it must have active descendants.
                (true, false) => {
                    self.split_neighborhood(coordinates, &nhood, min_neighbor_ptr, min_node_state)
                }
                // Node just became active, and none of its ancestors were active.
                (false, true) => {
                    // Make sure the neighborhood is loaded.
                    if min_is_loading || !self.neighborhood_is_loaded(&nhood) {
                        return None;
                    }

                    // At this point we've committed to meshing this node.
                    min_node_state.set_rendering();

                    if level == 0 {
                        // No descendants to merge.
                        return Some(LodChange::Spawn(nhood));
                    }

                    Some(self.merge_into_neighborhood(coordinates, nhood, min_neighbor_ptr))
                }
            }
        })
    }

    fn add_child_neighborhoods_to_heap(
        &mut self,
        parent_coords: IVec3,
        parent_nhood: &RenderNeighborhood,
        min_neighbor_ptr: NodePtr,
    ) {
        // Add all child neighborhoods to the heap.
        let child_neighborhoods =
            self.construct_child_neighborhoods(min_neighbor_ptr, &parent_nhood.neighbors);
        for (&child_offset, n) in CUBE_CORNERS.iter().zip(child_neighborhoods.into_iter()) {
            if let Some(child_neighborhood) = n {
                self.candidate_heap.push(RenderSearchNode::new(
                    child_neighborhood.level,
                    ChunkUnits(parent_coords + child_offset),
                    child_neighborhood.neighbors,
                    self.clip_sphere.map(|s| s.center),
                ));
            }
        }
    }

    fn split_neighborhood(
        &mut self,
        coords: IVec3,
        nhood: &RenderNeighborhood,
        min_neighbor_ptr: NodePtr,
        min_node_state: &NodeState,
    ) -> Option<LodChange> {
        // This chunk could potentially need to split over multiple levels, but we need to be careful not to run out
        // of render chunk budget. To be fair to other chunks in the queue that need to be split, we will only split
        // by one level for now.

        let child_neighborhoods =
            self.construct_child_neighborhoods(min_neighbor_ptr, &nhood.neighbors);

        // Make sure all child neighborhoods are loaded.
        for nhood in child_neighborhoods.iter().flatten() {
            if !self.neighborhood_is_loaded(nhood) {
                return None;
            }
        }

        // At this point, we've committed to meshing the children.
        min_node_state.clear_rendering();
        self.octree
            .visit_children(min_neighbor_ptr, |child_ptr, _| {
                let child_node = self.octree.get_value(child_ptr).unwrap();
                child_node.state().set_rendering();
                self.num_render_chunks += 1;
            });
        Some(LodChange::Split(Box::new(SplitChunk {
            old_chunk: NodeLocation::new(ChunkUnits(coords), min_neighbor_ptr),
            new_chunks: child_neighborhoods,
        })))
    }

    fn merge_into_neighborhood(
        &mut self,
        coords: IVec3,
        nhood: RenderNeighborhood,
        min_neighbor_ptr: NodePtr,
    ) -> LodChange {
        // This node might have active descendants. Merge those active descendants into this node.
        let mut deactivate_nodes = SmallVec::<[NodeLocation; 8]>::new();
        self.octree
            .visit_children(min_neighbor_ptr, |child_ptr, _| {
                self.octree
                    .visit_tree_depth_first(child_ptr, coords, 0, |node_ptr, node_coords| {
                        let descendant_node = self.octree.get_value(node_ptr).unwrap();
                        let descendant_was_active =
                            descendant_node.state().fetch_and_clear_rendering();
                        if descendant_was_active {
                            deactivate_nodes
                                .push(NodeLocation::new(ChunkUnits(node_coords), node_ptr));
                            VisitCommand::SkipDescendants
                        } else {
                            VisitCommand::Continue
                        }
                    })
            });

        self.num_render_chunks += 1;

        if deactivate_nodes.is_empty() {
            LodChange::Spawn(nhood)
        } else {
            LodChange::Merge(MergeChunks {
                old_chunks: deactivate_nodes,
                new_chunk: nhood,
            })
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
