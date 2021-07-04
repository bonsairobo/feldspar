use crate::{EditBuffer, SdfVoxelMap, VoxelType};

use building_blocks::{
    core::prelude::*,
    storage::{sled, ChunkDb3, ChunkKey, FastArrayCompressionNx2, FromBytesCompression, Lz4, Sd8},
};

pub struct VoxelWorldDb {
    chunks: SdfChunkDb,
}

pub type SdfChunkDb = ChunkDb3<FastArrayCompressionNx2<[i32; 3], Lz4, VoxelType, Sd8>>;

impl VoxelWorldDb {
    pub fn new(tree: sled::Tree) -> Self {
        Self {
            chunks: ChunkDb3::new(
                tree,
                FastArrayCompressionNx2::from_bytes_compression(Lz4 { level: 10 }),
            ),
        }
    }

    pub fn chunks(&self) -> &SdfChunkDb {
        &self.chunks
    }

    /// Loads all chunks present in the given superchunk `octant` into the `SdfVoxelMap` and marks them dirty.
    pub async fn load_superchunk_into_map<'a>(
        &self,
        octant: Octant,
        map: &mut SdfVoxelMap,
        edit_buffer: &mut EditBuffer,
    ) -> sled::Result<()> {
        let mut chunk_mins = Vec::new();
        self.chunks
            .read_chunks_in_orthant(0, octant, |key, chunk| {
                log::debug!("Inserting chunk {:?}", key);

                edit_buffer.mark_chunk_dirty(true, key.minimum);
                map.voxels.storage_mut().insert_chunk(key, chunk);
                chunk_mins.push(key.minimum);
            })
            .await?;

        if !chunk_mins.is_empty() {
            map.chunk_index.insert_superchunk(
                octant.minimum(),
                chunk_mins.into_iter().map(|min| ChunkKey::new(0, min)),
            );
        }

        Ok(())
    }
}
