mod node;
mod raycast;
mod streaming;

use crate::{
    ancestor_extent, chunk_bounding_sphere, chunk_extent_at_level_ivec3, descendant_extent,
    in_chunk_extent, sphere_intersecting_ancestor_chunk_extent, ChunkUnits, Sphere, VoxelUnits,
};

pub use grid_tree::{ChildIndex, FillCommand, Level, NodeKey, NodePtr, SlotState, VisitCommand};
pub use node::*;
pub use streaming::*;

use grid_tree::OctreeI32;
use ilattice::glam::IVec3;
use ilattice::prelude::Extent;

pub const CHILDREN: ChildIndex = OctreeI32::<()>::CHILDREN;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeLocation {
    pub coordinates: ChunkUnits<IVec3>,
    pub ptr: NodePtr,
}

impl NodeLocation {
    pub fn new(coordinates: ChunkUnits<IVec3>, ptr: NodePtr) -> Self {
        Self { coordinates, ptr }
    }
}

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
    pub stream_config: StreamingConfig,
}

impl ChunkClipMap {
    pub fn new(height: Level, stream_config: StreamingConfig) -> Self {
        Self {
            octree: OctreeI32::new(height),
            stream_config,
        }
    }

    /// Similar to `visit_extent_intersections`, but the callback can choose to fill any chunks that intersect `extent`.
    pub fn fill_extent_intersections(
        &mut self,
        min_level: Level,
        min_level_extent: VoxelUnits<Extent<IVec3>>,
        mut filler: impl FnMut(NodePtr, ChunkUnits<IVec3>, SlotState) -> FillCommand<ChunkNode>,
    ) {
        // Find the smallest extent at root level that covers the extent at the given level.
        let root_level = self.octree.root_level();
        let ChunkUnits(root_level_extent) =
            in_chunk_extent(min_level_extent).map(|e| ancestor_extent(root_level - min_level, e));

        // Recurse on each tree.
        for root_coords in root_level_extent.iter3() {
            if let FillCommand::Write(root_ptr, VisitCommand::Continue) =
                self.octree.fill_root(root_coords, |root_ptr, state| {
                    filler(
                        NodePtr::new(root_level, root_ptr),
                        ChunkUnits(root_coords),
                        state,
                    )
                })
            {
                let root_ptr = NodePtr::new(root_level, root_ptr);
                self.octree.fill_descendants(
                    root_ptr,
                    root_coords,
                    min_level,
                    |ptr, coords, state| {
                        let coords = ChunkUnits(coords);
                        let chunk_extent = chunk_extent_at_level_ivec3(ptr.level(), coords);
                        let min_level_chunk_extent =
                            chunk_extent.map(|e| descendant_extent(ptr.level() - min_level, e));
                        let VoxelUnits(intersecting) =
                            VoxelUnits::map2(min_level_chunk_extent, min_level_extent, |e1, e2| {
                                !e1.intersection(&e2).is_empty()
                            });

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

    /// NOTE: This only does sphere-on-sphere intersection tests, i.e. `lod0_sphere` vs the chunk node's bounding sphere. The
    /// chunks extents need not intersect.
    pub fn fill_sphere_intersections(
        &mut self,
        min_level: Level,
        lod0_sphere: VoxelUnits<Sphere>,
        mut filler: impl FnMut(NodePtr, ChunkUnits<IVec3>, SlotState) -> FillCommand<ChunkNode>,
    ) {
        let root_level = self.octree.root_level();
        let ChunkUnits(root_level_extent) =
            sphere_intersecting_ancestor_chunk_extent(lod0_sphere, root_level);

        // Recurse on each tree.
        for root_coords in root_level_extent.iter3() {
            let root_sphere = chunk_bounding_sphere(root_level, ChunkUnits(root_coords));
            if !VoxelUnits::map2(lod0_sphere, root_sphere, |s1, s2| s1.intersects(&s2)).into_inner()
            {
                continue;
            }

            if let FillCommand::Write(root_ptr, VisitCommand::Continue) =
                self.octree.fill_root(root_coords, |root_ptr, state| {
                    filler(
                        NodePtr::new(root_level, root_ptr),
                        ChunkUnits(root_coords),
                        state,
                    )
                })
            {
                let root_ptr = NodePtr::new(root_level, root_ptr);
                self.octree.fill_descendants(
                    root_ptr,
                    root_coords,
                    min_level,
                    |ptr, coords, state| {
                        let coords = ChunkUnits(coords);
                        let chunk_sphere = chunk_bounding_sphere(ptr.level(), coords);
                        if VoxelUnits::map2(chunk_sphere, lod0_sphere, |s1, s2| s1.intersects(&s2))
                            .into_inner()
                        {
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
        extent: VoxelUnits<Extent<IVec3>>,
        mut visitor: impl FnMut(NodePtr, ChunkUnits<IVec3>),
    ) {
        let root_level = self.octree.root_level();
        for (root_ptr, root_coords) in self.octree.iter_roots() {
            let root_extent = chunk_extent_at_level_ivec3(root_level, ChunkUnits(root_coords));
            let disjoint = VoxelUnits::map2(extent, root_extent, |e1, e2| {
                e1.intersection(&e2).is_empty()
            })
            .into_inner();
            if disjoint {
                continue;
            }
            self.octree
                .visit_tree_depth_first(root_ptr, root_coords, min_level, |ptr, coords| {
                    let coords = ChunkUnits(coords);
                    let this_extent = chunk_extent_at_level_ivec3(ptr.level(), coords);
                    let disjoint = VoxelUnits::map2(this_extent, extent, |e1, e2| {
                        e1.intersection(&e2).is_empty()
                    })
                    .into_inner();
                    if disjoint {
                        VisitCommand::SkipDescendants
                    } else {
                        visitor(ptr, coords);
                        VisitCommand::Continue
                    }
                })
        }
    }
}

// ████████╗███████╗███████╗████████╗
// ╚══██╔══╝██╔════╝██╔════╝╚══██╔══╝
//    ██║   █████╗  ███████╗   ██║
//    ██║   ██╔══╝  ╚════██║   ██║
//    ██║   ███████╗███████║   ██║
//    ╚═╝   ╚══════╝╚══════╝   ╚═╝

#[cfg(test)]
mod test {
    use super::node::NodeState;
    use super::*;
    use crate::glam::Vec3A;
    use crate::{chunk_extent_from_min_ivec3, in_chunk_extent, Chunk, NdView, Ray};

    use ndshape::Shape3i32;

    #[test]
    fn fill_extent() {
        let mut tree = ChunkClipMap::new(7, StreamingConfig::default());

        let write_min = VoxelUnits(IVec3::new(1, 2, 3));
        let write_extent = chunk_extent_from_min_ivec3(write_min);
        let ChunkUnits(chunks_extent) = in_chunk_extent(write_extent);

        // Fill in the extent with empty nodes and cache pointers to them.
        let mut node_pointers = NdView::new(
            vec![NodePtr::NULL; chunks_extent.volume() as usize],
            Shape3i32::new(chunks_extent.shape.to_array()),
        );
        tree.fill_extent_intersections(
            4,
            write_extent,
            |node_ptr, ChunkUnits(node_coords), _state| {
                if node_ptr.level() == 4 {
                    node_pointers[node_coords - chunks_extent.minimum] = node_ptr;
                }
                FillCommand::Write(ChunkNode::default(), VisitCommand::Continue)
            },
        );

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
    fn earliest_ray_intersection() {
        let mut tree = ChunkClipMap::new(3, StreamingConfig::default());

        // Insert just a single chunk at level 0.
        let write_key = NodeKey::new(0, IVec3::new(1, 1, 1));
        tree.octree
            .fill_path_to_node(write_key, |_node_ptr, _node_coords, _state| {
                FillCommand::Write(ChunkNode::default(), VisitCommand::Continue)
            });

        let (_ptr, coords, [tmin, tmax]) = tree
            .earliest_ray_intersection(VoxelUnits(Ray::new(Vec3A::ZERO, Vec3A::ONE)), 0)
            .unwrap();

        assert_eq!(coords, IVec3::new(1, 1, 1));
        assert_eq!(tmin, 1.0);
        assert_eq!(tmax, 17.0);
    }
}
