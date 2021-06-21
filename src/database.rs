use crate::VoxelType;

use building_blocks::storage::{ChunkDb3, FastArrayCompressionNx2, Lz4, Sd8};

pub struct VoxelWorldDb {
    chunks: SdfChunkDb,
}

pub type SdfChunkDb = ChunkDb3<FastArrayCompressionNx2<[i32; 3], Lz4, VoxelType, Sd8>>;
