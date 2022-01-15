use crate::clipmap::ChunkClipMap;
use crate::core::geometry::Sphere;
use crate::core::glam::{IVec3, Vec3A};
use crate::{
    clipmap::{ChunkNode, Level, NodeState, VisitCommand},
    coordinates::{
        chunk_bounding_sphere, sphere_intersecting_ancestor_chunk_extent, visit_children,
    },
    units::*,
};

use float_ord::FloatOrd;
use grid_tree::{AllocPtr, NodeKey, NodePtr};
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

    /// Searches for up to `budget` of the nodes marked as "loading." It is up to the caller to subsequently write or delete the
    /// data in the loading nodes so that they get marked as "loaded".
    pub fn near_phase_load_search(
        &self,
        budget: usize,
        observer: VoxelUnits<Vec3A>,
        mut rx: impl FnMut(Level, ChunkUnits<IVec3>),
    ) {
        let mut candidate_heap = BinaryHeap::new();
        let mut num_load_slots = 0;

        for (root_key, root_node) in self.octree.iter_roots() {
            candidate_heap.push(LoadSearchNode::new(
                root_key.level,
                ChunkUnits(root_key.coordinates),
                Some(root_node.self_ptr),
                None,
                observer,
            ));
        }

        while let Some(LoadSearchNode {
            level,
            coordinates,
            ptr,
            nearest_ancestor,
            ..
        }) = candidate_heap.pop()
        {
            if num_load_slots >= budget {
                break;
            }

            if level == 0 {
                // We hit LOD0 so this chunk needs to be loaded.
                rx(level, coordinates);
                num_load_slots += 1;

                // Mark the nearest ancestor as pending, assuming we don't have a node for loading level 0.
                let nearest_ancestor_ptr = nearest_ancestor.unwrap();
                let nearest_ancestor_node = self.octree.get_value(nearest_ancestor_ptr).unwrap();
                nearest_ancestor_node.state().set_load_pending();

                continue;
            }
            let child_level = level - 1;

            let node_entry = ptr.and_then(|p| {
                let node_ptr = NodePtr::new(level, p);
                self.octree.get_value(node_ptr).map(|n| (node_ptr, n))
            });
            if let Some((ptr, node)) = node_entry {
                if node.state().is_loading() && node.state().descendant_is_loading.none() {
                    // All descendants have loaded, so this slot is ready to be downsampled.
                    rx(level, coordinates);

                    // When the node is marked as loaded, we will clear this pending bit.
                    node.state().set_load_pending();

                    // Leaving this commented, we are choosing not to count LOD > 0 against the budget. Downsampling is much
                    // faster than generating LOD0, and there are many more LOD0 chunks, so it seems fair to just let as much
                    // downsampling happen as possible.
                    // num_load_slots += 1;

                    continue;
                }

                // If we're on a nonzero level, visit all children that need loading, regardless of which child nodes exist.
                if let Some(child_pointers) = self.octree.child_pointers(ptr) {
                    visit_children(coordinates.into_inner(), |child_index, child_coords| {
                        if !node.state().has_load_pending()
                            && node.state().descendant_is_loading.bit_is_set(child_index)
                        {
                            let child_ptr = child_pointers.get_child(child_index);
                            candidate_heap.push(LoadSearchNode::new(
                                child_level,
                                ChunkUnits(child_coords),
                                child_ptr.map(|p| p.alloc_ptr()),
                                Some(ptr),
                                observer,
                            ));
                        }
                    })
                }
            } else if level < self.stream_config.min_load_level {
                // We only recurse on missing nodes if we're under the load level. This is because when *any* new node is
                // inserted as a result of calling ChunkClipMap::insert_loading_node, all of its "descendant_is_loading" bits
                // are set. So during this search, we may get directed to empty nodes outside of the clip sphere by these bits
                // if we're at or above the load level. But below the load level, we want to search for any descendants of
                // load-level nodes that exist, i.e. load-level nodes that intersected the clip sphere.

                // We need to enumerate all child corners because this node doesn't exist, but we know it needs to be loaded.
                visit_children(coordinates.into_inner(), |_child_index, child_coords| {
                    candidate_heap.push(LoadSearchNode::new(
                        child_level,
                        ChunkUnits(child_coords),
                        None,
                        nearest_ancestor,
                        observer,
                    ));
                })
            }
        }
    }
}

#[derive(Clone, Copy)]
struct LoadSearchNode {
    level: Level,
    coordinates: ChunkUnits<IVec3>,
    closest_dist_to_observer: VoxelUnits<f32>,
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
            closest_dist_to_observer: VoxelUnits(closest_dist_to_observer),
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

impl Ord for LoadSearchNode {
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
