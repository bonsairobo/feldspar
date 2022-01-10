mod neighborhood_subdiv;
mod node;
mod raycast;
mod streaming;

use crate::chunk::CompressedChunk;
use crate::coordinates::{
    ancestor_extent, chunk_bounding_sphere, chunk_extent_at_level_ivec3, descendant_extent,
    in_chunk_extent, sphere_intersecting_ancestor_chunk_extent,
};
use crate::core::geometry::Sphere;
use crate::core::glam::IVec3;
use crate::core::ilattice::prelude::Extent;
use crate::units::{ChunkUnits, VoxelUnits};

pub use grid_tree::{
    BranchShape, ChildIndex, Level, NodeKey, NodePtr, OctreeShapeI32, VisitCommand, EMPTY_ALLOC_PTR,
};
pub use node::*;
pub use streaming::*;

use grid_tree::OctreeI32;

pub const CHILDREN: ChildIndex = OctreeI32::<()>::CHILDREN;
pub const CHILDREN_USIZE: usize = CHILDREN as usize;

pub type NodeEntry<'a> = grid_tree::NodeEntry<'a, ChunkNode, CHILDREN_USIZE>;

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
        mut filler: impl FnMut(NodeKey<IVec3>, &mut NodeEntry<'_>) -> VisitCommand,
    ) {
        // Find the smallest extent at root level that covers the extent at the given level.
        let root_level = self.octree.root_level();
        let ChunkUnits(root_level_extent) =
            in_chunk_extent(min_level_extent).map(|e| ancestor_extent(root_level - min_level, e));

        // Recurse on each tree.
        for root_coords in root_level_extent.iter3() {
            let root_key = NodeKey::new(root_level, root_coords);
            // We know that every root in root_level_extent intersects min_level_extent, so we only do intersection checks for
            // descendants.
            if let (Some(root_node), VisitCommand::Continue) = self
                .octree
                .fill_root(root_key, |entry| filler(root_key, entry))
            {
                let root_ptr = NodePtr::new(root_level, root_node.self_ptr);
                self.octree
                    .fill_descendants(root_ptr, root_coords, min_level, |key, entry| {
                        let chunk_extent =
                            chunk_extent_at_level_ivec3(key.level, ChunkUnits(key.coordinates));
                        let min_level_chunk_extent =
                            chunk_extent.map(|e| descendant_extent(key.level - min_level, e));
                        let VoxelUnits(intersecting) =
                            VoxelUnits::map2(min_level_chunk_extent, min_level_extent, |e1, e2| {
                                !e1.intersection(&e2).is_empty()
                            });

                        if intersecting {
                            filler(key, entry)
                        } else {
                            VisitCommand::SkipDescendants
                        }
                    })
            }
        }
    }

    /// NOTE: This only does sphere-on-sphere intersection tests, i.e. `lod0_sphere` vs the chunk node's bounding sphere. The
    /// chunks extents need not intersect.
    pub fn fill_sphere_intersections(
        &mut self,
        min_level: Level,
        lod0_sphere: VoxelUnits<Sphere>,
        mut filler: impl FnMut(NodeKey<IVec3>, &mut NodeEntry<'_>) -> VisitCommand,
    ) {
        let root_level = self.octree.root_level();
        let ChunkUnits(root_level_extent) =
            sphere_intersecting_ancestor_chunk_extent(lod0_sphere, root_level);

        // Recurse on each tree.
        for root_coords in root_level_extent.iter3() {
            let root_key = NodeKey::new(root_level, root_coords);
            self.octree
                .fill_tree_from_root(root_key, min_level, |node_key, entry| {
                    let coords = ChunkUnits(node_key.coordinates);
                    let chunk_sphere = chunk_bounding_sphere(node_key.level, coords);
                    let VoxelUnits(intersects) =
                        VoxelUnits::map2(chunk_sphere, lod0_sphere, |s1, s2| s1.intersects(&s2));
                    if intersects {
                        filler(root_key, entry)
                    } else {
                        VisitCommand::SkipDescendants
                    }
                });
        }
    }

    /// Visit octants in `level` that intersect `extent`.
    pub fn visit_extent_intersections(
        &self,
        min_level: Level,
        extent: VoxelUnits<Extent<IVec3>>,
        mut visitor: impl FnMut(NodePtr, ChunkUnits<IVec3>) -> VisitCommand,
    ) {
        let root_level = self.octree.root_level();
        for (root_key, root_node) in self.octree.iter_roots() {
            self.octree.visit_tree_depth_first(
                NodePtr::new(root_level, root_node.self_ptr),
                root_key.coordinates,
                min_level,
                |ptr, coords| {
                    let coords = ChunkUnits(coords);
                    let this_extent = chunk_extent_at_level_ivec3(ptr.level(), coords);
                    let disjoint = VoxelUnits::map2(this_extent, extent, |e1, e2| {
                        e1.intersection(&e2).is_empty()
                    })
                    .into_inner();
                    if disjoint {
                        VisitCommand::SkipDescendants
                    } else {
                        visitor(ptr, coords)
                    }
                },
            )
        }
    }

    pub fn insert_loading_node(&mut self, target_key: NodeKey<IVec3>) {
        let mut level_diff = self.octree.root_level() - target_key.level;
        self.octree.fill_path_to_node(target_key, |key, entry| {
            let (_node_ptr, node) = entry.or_insert_with(|| {
                let mut node = ChunkNode::default();
                node.state().set_loading();
                node.state_mut().descendant_is_loading.set_all();
                node
            });
            if level_diff == 0 {
                node.state_mut().descendant_is_loading.set_all();
                VisitCommand::SkipDescendants
            } else {
                level_diff -= 1;
                let child_coords =
                    OctreeShapeI32::ancestor_key(target_key.coordinates, level_diff as u32);
                let min_sibling = OctreeShapeI32::min_child_key(key.coordinates);
                let child_index = OctreeShapeI32::linearize_child(child_coords - min_sibling);
                node.state_mut().descendant_is_loading.set_bit(child_index);
                VisitCommand::Continue
            }
        })
    }

    pub fn fulfill_pending_load(
        &mut self,
        loaded_key: NodeKey<IVec3>,
        data: Option<CompressedChunk>,
    ) {
        todo!()
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
    use crate::core::{geometry::Ray, glam::Vec3A};
    use crate::{
        chunk::Chunk,
        coordinates::{chunk_extent_from_min_ivec3, in_chunk_extent},
        ndview::NdView,
    };

    use ndshape::RuntimeShape;

    #[test]
    fn fill_extent() {
        let mut tree = ChunkClipMap::new(7, StreamingConfig::default());

        let write_min = VoxelUnits(IVec3::new(1, 2, 3));
        let write_extent = chunk_extent_from_min_ivec3(write_min);
        let ChunkUnits(chunks_extent) = in_chunk_extent(write_extent);

        // Fill in the extent with empty nodes and cache pointers to them.
        let mut node_pointers = NdView::new(
            vec![NodePtr::new(0, EMPTY_ALLOC_PTR); chunks_extent.volume() as usize],
            RuntimeShape::<i32, 3>::new(chunks_extent.shape.to_array()),
        );
        tree.fill_extent_intersections(4, write_extent, |node_key, entry| {
            let (ptr, _value) = entry.or_insert_with(ChunkNode::default);
            if node_key.level == 4 {
                node_pointers[node_key.coordinates - chunks_extent.minimum] =
                    NodePtr::new(node_key.level, ptr);
            }
            VisitCommand::Continue
        });

        let mut num_nulls = 0;
        for &ptr in node_pointers.values.iter() {
            if ptr.is_null() {
                num_nulls += 1;
            }
        }
        assert_eq!(num_nulls, 0);

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
            .fill_path_to_node(write_key, |_node_key, entry| {
                entry.or_insert_with(ChunkNode::default);
                VisitCommand::Continue
            });

        let (_ptr, coords, [tmin, tmax]) = tree
            .earliest_ray_intersection(VoxelUnits(Ray::new(Vec3A::ZERO, Vec3A::ONE)), 0)
            .unwrap();

        assert_eq!(coords, IVec3::new(1, 1, 1));
        assert_eq!(tmin, 1.0);
        assert_eq!(tmax, 17.0);
    }
}
