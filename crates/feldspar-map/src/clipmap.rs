use crate::ChunkNode;

use grid_tree::OctreeI32;

/// An octree of [`ChunkNode`]s.
pub struct ChunkClipmap {
    pub octree: OctreeI32<ChunkNode>,
}

impl ChunkClipmap {}
