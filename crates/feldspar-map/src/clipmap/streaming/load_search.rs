use crate::clipmap::ChunkClipMap;
use crate::core::geometry::Sphere;
use crate::core::glam::{IVec3, Vec3A};
use crate::{
    clipmap::{ChunkNode, Level, NodeState, StreamingConfig, VisitCommand},
    coordinates::{
        chunk_bounding_sphere, sphere_intersecting_ancestor_chunk_extent, visit_children,
    },
    units::*,
};

use float_ord::FloatOrd;
use grid_tree::{AllocPtr, NodeKey, NodePtr, OctreeI32};
use std::cmp::Ordering;
use std::collections::BinaryHeap;

pub struct NodeSlot {
    pub coordinates: ChunkUnits<IVec3>,
    pub level: Level,
    pub dist: f32,
    pub is_render_candidate: bool,
}

impl NodeSlot {
    pub fn node_key(&self) -> NodeKey<IVec3> {
        NodeKey::new(self.level, self.coordinates.into_inner())
    }
}

impl ChunkClipMap {
    /// Searches for root nodes that entered the clip sphere this frame and inserts them as "load sentinel" nodes to be used by
    /// the near-phase search.
    pub fn broad_phase_load_search(
        &mut self,
        old_observer: VoxelUnits<Vec3A>,
        new_observer: VoxelUnits<Vec3A>,
    ) {
        let VoxelUnits(old_observer) = old_observer;
        let VoxelUnits(new_observer) = new_observer;
        let VoxelUnits(clip_radius) = self.stream_config.clip_sphere_radius;
        let root_level = self.octree.root_level();

        let old_clip_sphere = Sphere::new(old_observer, clip_radius);
        let new_clip_sphere = Sphere::new(new_observer, clip_radius);
        let ChunkUnits(new_root_level_extent) =
            sphere_intersecting_ancestor_chunk_extent(VoxelUnits(new_clip_sphere), root_level);

        for root_coords in new_root_level_extent.iter3() {
            let VoxelUnits(root_sphere) =
                chunk_bounding_sphere(root_level, ChunkUnits(root_coords));

            let dist_to_new_clip_sphere = root_sphere.center.distance(new_clip_sphere.center);
            let node_intersects_new_clip_sphere =
                dist_to_new_clip_sphere - root_sphere.radius < new_clip_sphere.radius;

            if !node_intersects_new_clip_sphere {
                return;
            }

            // Only insert if the node didn't already intersect the clip sphere.
            let dist_to_old_clip_sphere = root_sphere.center.distance(old_clip_sphere.center);
            let node_intersects_old_clip_sphere =
                dist_to_old_clip_sphere - root_sphere.radius < old_clip_sphere.radius;
            if !node_intersects_old_clip_sphere {
                let root_key = NodeKey::new(root_level, root_coords);
                self.octree.fill_root(root_key, |entry| {
                    entry.or_insert_with(|| ChunkNode::new_empty(NodeState::new_load_sentinel()));
                    VisitCommand::SkipDescendants
                });
            }
        }
    }

    pub fn near_phase_load_search(&self, observer: VoxelUnits<Vec3A>) -> NearPhaseLoadSearch<'_> {
        let mut candidate_heap = BinaryHeap::new();
        for (root_key, root_node) in self.octree.iter_roots() {
            candidate_heap.push(LoadSearchNode::new(
                root_key.level,
                ChunkUnits(root_key.coordinates),
                Some(root_node.self_ptr),
                None,
                observer,
            ));
        }
        NearPhaseLoadSearch {
            octree: &self.octree,
            config: self.stream_config,
            observer,
            candidate_heap,
            num_load_slots: 0,
        }
    }
}

/// Searches for nodes marked as "loading." It is up to the caller to subsequently complete the load and supply an
/// `Option<Chunk>`.
///
/// WARNING: Due to the internal use of atomics, it is safe but left unspecified what happens when two searches are run at the
/// same time on the same tree.
pub struct NearPhaseLoadSearch<'a> {
    octree: &'a OctreeI32<ChunkNode>,
    config: StreamingConfig,
    observer: VoxelUnits<Vec3A>,
    candidate_heap: BinaryHeap<LoadSearchNode>,
    num_load_slots: usize,
}

impl<'a> NearPhaseLoadSearch<'a> {
    pub fn is_done(&self) -> bool {
        self.candidate_heap.is_empty()
    }

    pub fn check_next_candidate(&mut self) -> Option<(NodeKey<IVec3>, Option<NodePtr>)> {
        self.candidate_heap.pop().and_then(|search_node| {
            let ptr_and_node = search_node.ptr.and_then(|p| {
                let node_ptr = NodePtr::new(search_node.level, p);
                self.octree.get_value(node_ptr).map(|n| (node_ptr, n))
            });
            if let Some((ptr, node)) = ptr_and_node {
                self.search_occupied_candidate(search_node, ptr, node)
            } else {
                self.search_vacant_candidate(search_node)
            }
        })
    }

    fn search_occupied_candidate(
        &mut self,
        search_node: LoadSearchNode,
        ptr: NodePtr,
        node: &ChunkNode,
    ) -> Option<(NodeKey<IVec3>, Option<NodePtr>)> {
        if node.state().has_load_pending() {
            // Don't start a redundant load.
            return None;
        }

        let LoadSearchNode {
            level,
            coordinates,
            nearest_ancestor,
            center_dist_to_observer: VoxelUnits(center_dist_to_observer),
            bounding_sphere: VoxelUnits(bounding_sphere),
            ..
        } = search_node;

        let VoxelUnits(detail) = self.config.detail;
        // PERF: we at least need to load the parent of an active node for LOD blending, but this condition may also load more
        // ancestors than necessary; this could be lazier.
        let do_load = level == 0
            || (node.state().is_loading() && node.state().descendant_is_loading.none())
            || center_dist_to_observer / bounding_sphere.radius > detail;

        if do_load {
            // When the node is marked as loaded, we will clear this pending bit.
            node.state().set_load_pending();
            self.num_load_slots += 1;
            return Some((
                NodeKey::new(level, coordinates.into_inner()),
                nearest_ancestor,
            ));
        }

        // If we're on a nonzero level, visit all children that need loading, regardless of which child nodes exist.
        if let Some(child_pointers) = self.octree.child_pointers(ptr) {
            let child_level = level - 1;
            visit_children(coordinates.into_inner(), |child_index, child_coords| {
                if node.state().descendant_is_loading.bit_is_set(child_index) {
                    let child_ptr = child_pointers.get_child(child_index);
                    self.candidate_heap.push(LoadSearchNode::new(
                        child_level,
                        ChunkUnits(child_coords),
                        child_ptr.map(|p| p.alloc_ptr()),
                        Some(ptr),
                        self.observer,
                    ));
                }
            })
        }

        None
    }

    fn search_vacant_candidate(
        &mut self,
        search_node: LoadSearchNode,
    ) -> Option<(NodeKey<IVec3>, Option<NodePtr>)> {
        let LoadSearchNode {
            level,
            coordinates,
            nearest_ancestor,
            center_dist_to_observer: VoxelUnits(center_dist_to_observer),
            bounding_sphere: VoxelUnits(bounding_sphere),
            ..
        } = search_node;

        let VoxelUnits(detail) = self.config.detail;
        let do_load = level == 0 || center_dist_to_observer / bounding_sphere.radius > detail;

        if do_load {
            // Mark the nearest ancestor as pending. All vacant candidates must have an existing ancestor node, as guaranteed by
            // the broad phase load search.
            let ancestor_ptr = nearest_ancestor.unwrap();
            let nearest_ancestor_node = self.octree.get_value(ancestor_ptr).unwrap();
            nearest_ancestor_node.state().set_load_pending();
            self.num_load_slots += 1;
            return Some((
                NodeKey::new(level, coordinates.into_inner()),
                nearest_ancestor,
            ));
        }

        // We need to enumerate all child corners because this node doesn't exist, but we know it needs to be loaded.
        let child_level = level - 1;
        visit_children(coordinates.into_inner(), |_child_index, child_coords| {
            self.candidate_heap.push(LoadSearchNode::new(
                child_level,
                ChunkUnits(child_coords),
                None,
                nearest_ancestor,
                self.observer,
            ));
        });
        None
    }
}

impl<'a> Iterator for NearPhaseLoadSearch<'a> {
    type Item = (NodeKey<IVec3>, Option<NodePtr>);

    fn next(&mut self) -> Option<Self::Item> {
        let mut next_find = None;
        while !self.is_done() {
            next_find = self.check_next_candidate();
        }
        next_find
    }
}

#[derive(Clone, Copy)]
struct LoadSearchNode {
    level: Level,
    coordinates: ChunkUnits<IVec3>,
    center_dist_to_observer: VoxelUnits<f32>,
    closest_dist_to_observer: VoxelUnits<f32>,
    bounding_sphere: VoxelUnits<Sphere>,
    // Optional because we might search into vacant space.
    ptr: Option<AllocPtr>,
    nearest_ancestor: Option<NodePtr>,
}

impl LoadSearchNode {
    fn new(
        level: Level,
        coordinates: ChunkUnits<IVec3>,
        ptr: Option<AllocPtr>,
        nearest_ancestor: Option<NodePtr>,
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
            ptr,
            nearest_ancestor,
            center_dist_to_observer: VoxelUnits(center_dist_to_observer),
            closest_dist_to_observer: VoxelUnits(closest_dist_to_observer),
            bounding_sphere: VoxelUnits(bounding_sphere),
        }
    }
}

impl PartialEq for LoadSearchNode {
    fn eq(&self, other: &Self) -> bool {
        self.level == other.level && self.coordinates == other.coordinates
    }
}
impl Eq for LoadSearchNode {}

impl PartialOrd for LoadSearchNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        VoxelUnits::map2(
            self.closest_dist_to_observer,
            other.closest_dist_to_observer,
            |d1, d2| FloatOrd(d1).partial_cmp(&FloatOrd(d2)),
        )
        .into_inner()
        .map(Ordering::reverse)
    }
}

impl Ord for LoadSearchNode {
    fn cmp(&self, other: &Self) -> Ordering {
        VoxelUnits::map2(
            self.closest_dist_to_observer,
            other.closest_dist_to_observer,
            |d1, d2| FloatOrd(d1).cmp(&FloatOrd(d2)),
        )
        .into_inner()
        .reverse()
    }
}
