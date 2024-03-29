mod neighborhood_subdiv;
mod node;
mod raycast;
mod streaming;

use crate::chunk::CompressedChunk;
use crate::coordinates::{
    ancestor_extent, child_index, chunk_bounding_sphere, chunk_extent_at_level_ivec3,
    descendant_extent, in_chunk_extent, sphere_intersecting_ancestor_chunk_extent,
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
use smallvec::SmallVec;

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

    /// Tries to collapse nodes with the same homogeneous value, starting from `key` and working up the line of ancestors.
    pub fn try_collapse_key(&mut self, key: NodeKey<IVec3>) {
        // NOTE: We can't collapse nodes with a load pending!
        todo!()
    }

    /// # Load vs Edit Conflict Resolution
    ///
    /// Asynchronous loads and edits can cause a scenario where an edit overlaps a region with a pending load. Because the edit
    /// is necessarily newer information, it will clear the "load pending" bit and take precedence. When the load is completed,
    /// it will check if the load is still pending; if not, the loaded data gets ignored and dropped.
    ///
    /// Similarly, if the `nearest_ancestor` is empty, the load is canceled.
    pub fn complete_pending_load(&mut self, load: PendingLoad) {
        // We need to ensure a couple things:
        // 1. If we insert a `None`, then we need to check if we're the last sibling node finished loading and maybe collapse
        //    into the parent node if all children are empty. This continues recursively to the root, but we will leave empty
        //    roots so we at least know they are loaded.
        // 2. A very similar process needs to happen for the `descendant_is_loading` bits. The last child loaded checks if the
        //    grandparent is entirely loaded, etc.
        //
        // Structurally, this algorithm finds the path to `loaded_key` and then retraces that path backwards in order to fix up
        // ancestors.

        let PendingLoad {
            loaded_key,
            link_ptr,
            chunk,
        } = load;

        let mut do_collapse = false;
        match link_ptr {
            LinkPointer::OverwriteNode { child, parent } => {
                // The node existed when it started loading. It's guaranteed to exist when the load completes because other
                // users are not allowed to remove the node when it has a load pending.
                let node = self.octree.get_value_mut(child).unwrap();

                // We rely on &mut borrow to guarantee that no one else clears this bit.
                assert!(node.state_mut().fetch_and_clear_load_pending());

                let was_loading = node.state_mut().fetch_and_clear_loading();
                if !was_loading {
                    // This means there was an intervening edit. Cancel the load.
                    return;
                }

                if let Some(chunk) = chunk {
                    node.put_compressed(chunk);
                } else {
                    node.take_chunk();
                }

                // If this is the last load of this subtree, then clear the descendant is loading bit on the parent.
                if node.state().descendant_is_loading.none() {
                    if let Some(parent_ptr) = parent {
                        let parent_node = self.octree.get_value_mut(parent_ptr).unwrap();
                        let descendant_index = child_index(loaded_key.coordinates);
                        parent_node
                            .state_mut()
                            .descendant_is_loading
                            .clear_bit(descendant_index);
                        if parent_node.state().descendant_is_loading.none() {
                            do_collapse = true;
                        }
                    }
                }
            }
            LinkPointer::LinkToNearestAncestor(nearest_ancestor_ptr) => {
                let ancestor_node =
                    if let Some(ancestor_node) = self.octree.get_value(nearest_ancestor_ptr) {
                        if ancestor_node.state().fetch_and_clear_load_pending() {
                            ancestor_node
                        } else {
                            // Cancel load.
                            return;
                        }
                    } else {
                        // Cancel load.
                        return;
                    };

                // We need to link a new node to the ancestor.
                assert!(nearest_ancestor_ptr.level() > loaded_key.level);
                let level_diff = nearest_ancestor_ptr.level() - loaded_key.level;

                let nearest_ancestor_coords = loaded_key.coordinates << level_diff;
                let mut path = SmallVec::<[NodePtr; 32]>::new();
                self.octree.fill_path_to_node(
                    nearest_ancestor_coords,
                    nearest_ancestor_ptr,
                    loaded_key,
                    |key, entry| {
                        let (ptr, node) =
                            entry.or_insert_with(|| ChunkNode::new_empty(NodeState::new_loading()));
                        path.push(NodePtr::new(key.level, ptr));
                        VisitCommand::Continue
                    },
                );

                // Check if this was the last sibling loaded, maybe collapse.
                todo!()
            }
        }

        if do_collapse {
            // PERF: most expensive path. We need to start from the root for collapsing an arbitrary number of levels.
            self.try_collapse_key(loaded_key);
        }
    }
}

pub struct PendingLoad {
    pub loaded_key: NodeKey<IVec3>,
    pub link_ptr: LinkPointer,
    pub chunk: Option<CompressedChunk>,
}

pub enum LinkPointer {
    LinkToNearestAncestor(NodePtr),
    OverwriteNode {
        child: NodePtr,
        parent: Option<NodePtr>,
    },
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
            let (ptr, _value) =
                entry.or_insert_with(|| ChunkNode::new_empty(NodeState::new_zeroed()));
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
                ChunkNode::new_compressed(Chunk::default().compress(), NodeState::new_zeroed());
        }
    }

    #[test]
    fn earliest_ray_intersection() {
        let mut tree = ChunkClipMap::new(3, StreamingConfig::default());

        // Insert just a single chunk at level 0.
        let write_key = NodeKey::new(0, IVec3::new(1, 1, 1));
        tree.octree
            .fill_path_to_node_from_root(write_key, |_node_key, entry| {
                entry.or_insert_with(|| ChunkNode::new_empty(NodeState::new_zeroed()));
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
