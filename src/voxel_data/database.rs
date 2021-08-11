use crate::prelude::{FrameMapChanges, SdfArray, SdfArrayCompression};

use building_blocks::{
    core::prelude::*,
    storage::{
        database::sled,
        prelude::{ChunkKey3, ReadableChunkDb, VersionedChunkDb3},
    },
};

/// The database storing all voxel chunks.
pub struct VoxelDb {
    chunks: SdfChunkDb,
}

pub type SdfChunkDb = VersionedChunkDb3<SdfArrayCompression>;

impl VoxelDb {
    pub fn new(chunks: SdfChunkDb) -> Self {
        Self { chunks }
    }

    pub fn chunks(&self) -> &SdfChunkDb {
        &self.chunks
    }

    /// Loads all chunks present in the given superchunk `octant` into the `FrameMapChanges` and marks them and their neighbors
    /// dirty. These chunks will be processed alongside the edits to keep the data pipeline more consistent.
    pub async fn load_superchunk_into_change_buffer<'a>(
        &self,
        octant: Octant,
        change_buffer: &mut FrameMapChanges,
    ) -> sled::Result<()> {
        let read_result = self.chunks.read_chunks_in_orthant(0, octant)?;
        let mut chunks = Vec::new();
        read_result
            .decompress(|key, chunk| {
                chunks.push((key, chunk));
            })
            .await;

        if !chunks.is_empty() {
            change_buffer.load_superchunk(LoadedSuperChunk { octant, chunks });
        }

        Ok(())
    }
}

pub struct LoadedSuperChunk {
    pub octant: Octant,
    pub chunks: Vec<(ChunkKey3, SdfArray)>,
}
