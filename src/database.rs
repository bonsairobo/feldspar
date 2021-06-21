use crate::VoxelType;

use building_blocks::storage::{
    sled, ChunkDb3, FastArrayCompressionNx2, FromBytesCompression, Lz4, Sd8,
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
}
