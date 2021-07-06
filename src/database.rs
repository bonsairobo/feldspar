use crate::{ChangeBuffer, SdfArray, SdfArrayCompression};

use building_blocks::{
    core::prelude::*,
    storage::{sled, ChunkDb3, ChunkKey3, FromBytesCompression, Lz4},
};

pub struct VoxelWorldDb {
    chunks: SdfChunkDb,
}

pub type SdfChunkDb = ChunkDb3<SdfArrayCompression>;

impl VoxelWorldDb {
    pub fn new(tree: sled::Tree) -> Self {
        Self {
            chunks: ChunkDb3::new(
                tree,
                SdfArrayCompression::from_bytes_compression(Lz4 { level: 10 }),
            ),
        }
    }

    pub fn chunks(&self) -> &SdfChunkDb {
        &self.chunks
    }

    /// Loads all chunks present in the given superchunk `octant` into the `ChangeBuffer` and marks them and their neighbors
    /// dirty. These chunks will be processed alongside the edits to keep the data pipeline more consistent.
    pub async fn load_superchunk_into_change_buffer<'a>(
        &self,
        octant: Octant,
        change_buffer: &mut ChangeBuffer,
    ) -> sled::Result<()> {
        let mut chunks = Vec::new();
        self.chunks
            .read_chunks_in_orthant(0, octant, |key, chunk| {
                chunks.push((key, chunk));
            })
            .await?;

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
