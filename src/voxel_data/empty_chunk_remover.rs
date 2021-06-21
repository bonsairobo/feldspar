use crate::SdfVoxelMap;

use bevy::ecs::prelude::*;
use building_blocks::storage::ChunkKey3;

/// The resource that tracks which chunks recently became empty and should be removed. This enables multiple methods of
/// detecting empty chunks. Chunks will be removed at the end of the frame in which they are marked as empty, but removal
/// happens before the edit buffer is merged into the `SdfVoxelMap`, so writes from the same frame will not be removed.
#[derive(Default)]
pub struct EmptyChunks {
    chunks_to_remove: Vec<ChunkKey3>,
}

impl EmptyChunks {
    /// Mark the chunk at `chunk_key` as "empty" and thus ready to be removed by the `empty_chunk_remover_system`.
    pub fn mark_for_removal(&mut self, chunk_key: ChunkKey3) {
        self.chunks_to_remove.push(chunk_key);
    }
}

pub fn empty_chunk_remover_system(
    mut empty_chunks: ResMut<EmptyChunks>,
    mut voxel_map: ResMut<SdfVoxelMap>,
) {
    for chunk_key in empty_chunks.chunks_to_remove.drain(..) {
        voxel_map.voxels.storage_mut().remove(chunk_key);
    }
}
