use crate::{ChunkNode, Ray, CHUNK_SHAPE_IVEC3, CHUNK_SHAPE_LOG2_IVEC3};

use grid_tree::{FillCommand, Level, NodePtr, OctreeI32, SlotState, VisitCommand};
use ilattice::glam::{IVec3, Vec3A};
use ilattice::prelude::Extent;

/// An octree of [`ChunkNode`]s.
///
/// Each node is separately synchonized with atomics and `RwLock`s, so mixed read/write of existing nodes is possible. However,
/// inserting and removing nodes requires mutual exlusion.
///
/// Being a tall tree, it works best if operations are done in phases while limiting tree walks. The optimal frame-based
/// workload includes separate read, write, and compress phases, in that order. Loads happen asynchronously, but are ultimately
/// merged into the tree during the write phase, if they are still relevant. Readers only participate in the read phase, and
/// they can efficiently decompress chunks inline with minimal synchronization. Editors will participate in both the read and
/// write phases; first reading and writing out of place, then merging the edited copy during the write phase. Finally, chunks
/// can be compressed in parallel.
pub struct ChunkClipMap {
    pub octree: OctreeI32<ChunkNode>,
}

impl ChunkClipMap {
    pub fn new(height: Level) -> Self {
        Self {
            octree: OctreeI32::new(height),
        }
    }

    /// Similar to `visit_extent_intersections`, but the callback can choose to fill any chunks that intersect `extent`.
    pub fn fill_extent_intersections(
        &mut self,
        min_level: Level,
        extent: Extent<IVec3>,
        mut filler: impl FnMut(NodePtr, IVec3, SlotState) -> FillCommand<ChunkNode>,
    ) {
        // Find the smallest extent at root level that covers the extent at the given level.
        let root_level = self.octree.root_level();
        let root_level_extent = ancestor_extent(root_level - min_level, extent);

        // Recurse on each tree.
        for root_coords in root_level_extent.iter3() {
            if let FillCommand::Write(root_ptr, VisitCommand::Continue) =
                self.octree.fill_root(root_coords, |root_ptr, state| {
                    filler(NodePtr::new(root_level, root_ptr), root_coords, state)
                })
            {
                // TODO: call filler on root
                let root_ptr = self
                    .octree
                    .get_or_create_root(root_coords, ChunkNode::default);
                self.octree.fill_descendants(
                    root_ptr,
                    root_coords,
                    min_level,
                    |ptr, coords, state| {
                        let chunk_extent = chunk_extent_ivec3(ptr.level(), coords);
                        let extent_at_level =
                            descendant_extent(ptr.level() - min_level, chunk_extent);
                        let intersecting = !extent_at_level.intersection(&extent).is_empty();

                        if intersecting {
                            filler(ptr, coords, state)
                        } else {
                            FillCommand::SkipDescendants
                        }
                    },
                )
            }
        }
    }

    /// Visit octants in `level` that intersect `extent`.
    pub fn visit_extent_intersections(
        &self,
        min_level: Level,
        extent: Extent<IVec3>,
        mut visitor: impl FnMut(NodePtr, IVec3),
    ) {
        let root_level = self.octree.root_level();
        for (root_ptr, root_coords) in self.octree.iter_roots() {
            let root_extent = chunk_extent_ivec3(root_level, root_coords);
            if extent.intersection(&root_extent).is_empty() {
                continue;
            }
            self.octree
                .visit_tree_depth_first(root_ptr, root_coords, min_level, |ptr, coords| {
                    let this_extent = chunk_extent_ivec3(ptr.level(), coords);
                    if this_extent.intersection(&extent).is_empty() {
                        VisitCommand::SkipDescendants
                    } else {
                        visitor(ptr, coords);
                        VisitCommand::Continue
                    }
                })
        }
    }

    /// Visit [`NodePtr`]s breadth-first, skipping octants than don't intersect `ray`.
    ///
    /// Going breadth-first is more fair if the search needs to be terminated early.
    pub fn visit_ray_intersections(
        &self,
        ray: Ray,
        min_level: Level,
        mut visitor: impl FnMut(NodePtr, IVec3, Extent<Vec3A>, [f32; 2]) -> VisitCommand,
    ) {
        for (root_ptr, root_coords) in self.octree.iter_roots() {
            let extent = chunk_extent_vec3a(root_ptr.level(), root_coords);
            if let Some(time_window) = ray.cast_at_extent(extent) {
                self.octree.visit_tree_breadth_first(
                    root_ptr,
                    root_coords,
                    min_level,
                    |ptr, coords| {
                        let extent = chunk_extent_vec3a(ptr.level(), coords);
                        if let Some(time_window) = ray.cast_at_extent(extent) {
                            visitor(ptr, coords, extent, time_window)
                        } else {
                            VisitCommand::SkipDescendants
                        }
                    },
                );
            }
        }
    }

    pub fn earliest_ray_intersection(
        &self,
        ray: Ray,
        min_level: Level,
    ) -> Option<(NodePtr, IVec3, [f32; 2])> {
        let mut earliest_window = [f32::INFINITY; 2];
        let mut earliest_ptr = None;
        let mut earliest_coords = IVec3::ZERO;
        self.visit_ray_intersections(ray, min_level, |ptr, coords, _aabb, window| {
            // Take the intersection window that started earliest and has the lowest level.
            if window[0] < earliest_window[0]
                && earliest_ptr
                    .map(|ep: NodePtr| ptr.level() <= ep.level())
                    .unwrap_or(true)
            {
                earliest_ptr = Some(ptr);
                earliest_coords = coords;
                earliest_window = window;
            }
            VisitCommand::Continue
        });

        (earliest_window[1] >= earliest_window[0])
            .then(|| (earliest_ptr.unwrap(), earliest_coords, earliest_window))
    }
}

pub fn ancestor_extent(levels_up: Level, extent: Extent<IVec3>) -> Extent<IVec3> {
    // We need the minimum to be an ancestor of (cover) the minimum.
    // We need the maximum to be an ancestor of (cover) the maximum.
    Extent::from_min_and_max(extent.minimum >> levels_up, extent.max() >> levels_up)
}

pub fn descendant_extent(levels_down: Level, extent: Extent<IVec3>) -> Extent<IVec3> {
    // Minimum and shape are simply multiplied.
    extent << levels_down
}

pub fn chunk_extent_vec3a(level: Level, coordinates: IVec3) -> Extent<Vec3A> {
    chunk_extent_ivec3(level, coordinates).map_components(|c| c.as_vec3a())
}

/// The extent in voxel coordinates of the chunk found at `(level, chunk coordinates)`.
pub fn chunk_extent_ivec3(level: Level, coordinates: IVec3) -> Extent<IVec3> {
    let min = coordinates << level;
    let shape = CHUNK_SHAPE_IVEC3 << level;
    Extent::from_min_and_shape(min, shape)
}

/// Transforms a world-space extent `e` into a chunk-space extent `e'` that contains the coordinates of all chunks intersected
/// by `e`.
pub fn in_chunk_extent(e: Extent<IVec3>) -> Extent<IVec3> {
    Extent::from_min_and_max(
        e.minimum >> CHUNK_SHAPE_LOG2_IVEC3,
        e.max() >> CHUNK_SHAPE_LOG2_IVEC3,
    )
}

/// Returns the "chunk coordinates" of the chunk that contains `p`.
pub fn in_chunk(p: IVec3) -> IVec3 {
    p >> CHUNK_SHAPE_LOG2_IVEC3
}

// ████████╗███████╗███████╗████████╗
// ╚══██╔══╝██╔════╝██╔════╝╚══██╔══╝
//    ██║   █████╗  ███████╗   ██║
//    ██║   ██╔══╝  ╚════██║   ██║
//    ██║   ███████╗███████║   ██║
//    ╚═╝   ╚══════╝╚══════╝   ╚═╝

#[cfg(test)]
mod test {
    use super::*;
    use crate::{Chunk, NdView, NodeState, Ray};

    use grid_tree::NodeKey;
    use ndshape::Shape3i32;

    #[test]
    fn fill_extent() {
        let mut tree = ChunkClipMap::new(7);

        let write_min = IVec3::new(1, 2, 3);
        let write_extent = Extent::from_min_and_shape(write_min, CHUNK_SHAPE_IVEC3);
        let chunks_extent = in_chunk_extent(write_extent);

        // Fill in the extent with empty nodes and cache pointers to them.
        let mut node_pointers = NdView::new(
            vec![NodePtr::NULL; chunks_extent.volume() as usize],
            Shape3i32::new(chunks_extent.shape.to_array()),
        );
        tree.fill_extent_intersections(4, write_extent, |node_ptr, node_coords, _state| {
            if node_ptr.level() == 4 {
                node_pointers[node_coords - chunks_extent.minimum] = node_ptr;
            }
            FillCommand::Write(ChunkNode::default(), VisitCommand::Continue)
        });

        for &ptr in node_pointers.values.iter() {
            assert_ne!(ptr, NodePtr::NULL);
        }

        // Now go back and write new chunks in O(chunks) instead of searching for each node.
        for p in chunks_extent.iter3() {
            let ptr = node_pointers[p];
            *tree.octree.get_value_mut(ptr).unwrap() =
                ChunkNode::new_compressed(Chunk::default().compress(), NodeState::default());
        }
    }

    #[test]
    fn visit_ray_intersection() {
        let mut tree = ChunkClipMap::new(3);

        // Insert just a single chunk at level 0.
        let write_key = NodeKey::new(0, IVec3::new(1, 1, 1));
        tree.octree
            .fill_path_to_node(write_key, |node_ptr, node_coords, _state| {
                FillCommand::Write(ChunkNode::default(), VisitCommand::Continue)
            });

        let mut intersections = Vec::new();
        tree.visit_ray_intersections(
            Ray::new(Vec3A::ZERO, Vec3A::ONE),
            0,
            |ptr, coords, aabb, time_window| {
                intersections.push((ptr.level(), coords, aabb, time_window));
                VisitCommand::Continue
            },
        );

        assert_eq!(
            intersections.as_slice(),
            &[
                (
                    2,
                    IVec3::new(0, 0, 0),
                    Extent::from_min_and_shape(Vec3A::ZERO, Vec3A::splat(64.0)),
                    [0.0, 64.0]
                ),
                (
                    1,
                    IVec3::new(0, 0, 0),
                    Extent::from_min_and_shape(Vec3A::ZERO, Vec3A::splat(32.0)),
                    [0.0, 32.0]
                ),
                (
                    0,
                    IVec3::new(1, 1, 1),
                    Extent::from_min_and_shape(Vec3A::splat(1.0), Vec3A::splat(16.0)),
                    [1.0, 17.0]
                ),
            ]
        );
    }
}
