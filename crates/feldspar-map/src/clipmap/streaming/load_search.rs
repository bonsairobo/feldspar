use super::StreamingConfig;
use crate::clipmap::ChunkClipMap;
use crate::core::geometry::Sphere;
use crate::core::glam::{IVec3, Vec3A};
use crate::{
    clipmap::Level,
    coordinates::{
        chunk_bounding_sphere, sphere_intersecting_ancestor_chunk_extent, visit_children,
    },
    units::*,
};

use float_ord::FloatOrd;
use grid_tree::{AllocPtr, NodePtr};
use std::collections::BinaryHeap;

pub struct NodeSlot {
    pub coordinates: ChunkUnits<IVec3>,
    pub level: Level,
    pub dist: f32,
    pub is_render_candidate: bool,
}

impl ChunkClipMap {
    /// Searches for up to `budget` of the nodes marked as "loading." It is up to the caller to subsequently write or delete the
    /// data in the loading nodes so that they get marked as "loaded".
    pub fn loading_nodes(
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
                observer,
            ));
        }

        while let Some(LoadSearchNode {
            level,
            coordinates,
            ptr,
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

                    // Leaving this commented, we are choosing not to count LOD > 0 against the budget. Downsampling is much
                    // faster than generating LOD0, and there are many more LOD0 chunks, so it seems fair to just let as much
                    // downsampling happen as possible.
                    // num_load_slots += 1;

                    continue;
                }

                // If we're on a nonzero level, visit all children that need loading, regardless of which child nodes exist.
                if let Some(child_pointers) = self.octree.child_pointers(ptr) {
                    visit_children(coordinates.into_inner(), |child_index, child_coords| {
                        if node.state().descendant_is_loading.bit_is_set(child_index) {
                            let child_ptr = child_pointers.get_child(child_index);
                            candidate_heap.push(LoadSearchNode::new(
                                child_level,
                                ChunkUnits(child_coords),
                                child_ptr.map(|p| p.alloc_ptr()),
                                observer,
                            ));
                        }
                    })
                }
            } else {
                // We need to enumerate all child corners because this node doesn't exist, but we know it needs to be
                // loaded.
                visit_children(coordinates.into_inner(), |_child_index, child_coords| {
                    candidate_heap.push(LoadSearchNode::new(
                        child_level,
                        ChunkUnits(child_coords),
                        None,
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
}

impl LoadSearchNode {
    fn new(
        level: Level,
        coordinates: ChunkUnits<IVec3>,
        ptr: Option<AllocPtr>,
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

pub fn new_nodes_intersecting_sphere(
    config: StreamingConfig,
    root_level: Level,
    old_observer: VoxelUnits<Vec3A>,
    new_observer: VoxelUnits<Vec3A>,
    mut rx: impl FnMut(NodeSlot),
) {
    let old_clip_sphere = VoxelUnits::map2(old_observer, config.clip_sphere_radius, Sphere::new);
    let new_clip_sphere = VoxelUnits::map2(new_observer, config.clip_sphere_radius, Sphere::new);
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
    let VoxelUnits(detail) = config.detail;
    let VoxelUnits(old_clip_sphere) = old_clip_sphere;
    let VoxelUnits(new_clip_sphere) = new_clip_sphere;
    let VoxelUnits(node_sphere) = chunk_bounding_sphere(node_level, node_coords);

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
                node_level == 0 || dist_to_new_clip_sphere / node_sphere.radius > detail;

            rx(NodeSlot {
                coordinates: node_coords,
                level: node_level,
                dist: dist_to_new_clip_sphere,
                is_render_candidate,
            });
        }
    }
}
