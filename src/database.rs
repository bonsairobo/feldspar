use crate::{SdfVoxelMap, VoxelType};

use building_blocks::{
    core::prelude::*,
    storage::{sled, ChunkDb3, FastArrayCompressionNx2, FromBytesCompression, Lz4, Sd8},
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

    pub async fn load_chunks_into_map(
        &self,
        lod: u8,
        extent: Extent3i,
        map: &mut SdfVoxelMap,
    ) -> sled::Result<()> {
        // Heuristic: We want the orthant edge length to be about 1/2 the extent's smallest dimension.
        let orthant_edge_length = extent.shape.min_component() >> 1;
        let orthant_exponent = orthant_edge_length.trailing_zeros() as i32;
        self.chunks
            .read_orthants_covering_extent(lod, orthant_exponent, extent, |key, chunk| {
                map.voxels.write_chunk(key, chunk)
            })
            .await
    }
}
