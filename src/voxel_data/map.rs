use crate::{
    ThreadLocalResource, ThreadLocalResourceHandle, VoxelType, VoxelTypeInfo,
    EMPTY_SIGNED_DISTANCE, EMPTY_VOXEL_TYPE,
};

use building_blocks::prelude::*;

pub struct SdfVoxelMap {
    pub voxels: CompressibleSdfChunkMap,
    pub palette: SdfVoxelPalette,
}

impl SdfVoxelMap {
    pub fn new_empty(chunk_shape: Point3i) -> Self {
        Self {
            voxels: empty_compressible_sdf_chunk_map(chunk_shape),
            palette: SdfVoxelPalette::new_empty(),
        }
    }

    /// Returns a closure that transforms voxels into their type's corresponding info. This is intended to be used with a
    /// `TransformMap`.
    #[inline]
    pub fn voxel_info_transform<'a>(&'a self) -> impl Fn((VoxelType, Sd8)) -> &'a VoxelTypeInfo {
        move |(v_type, _dist): (VoxelType, Sd8)| self.palette.get_voxel_type_info(v_type)
    }

    pub fn reader<'a>(
        &'a self,
        handle: &'a ThreadLocalResourceHandle<SdfChunkCache>,
    ) -> CompressibleSdfChunkMapReader {
        let local_cache = handle.get_or_create_with(SdfChunkCache::new);

        self.voxels.reader(local_cache)
    }
}

#[derive(Clone, Default)]
pub struct SdfVoxelPalette {
    infos: Vec<VoxelTypeInfo>,
}

impl SdfVoxelPalette {
    pub fn new_empty() -> Self {
        Self { infos: Vec::new() }
    }

    pub fn get_voxel_type_info(&self, voxel: VoxelType) -> &VoxelTypeInfo {
        &self.infos[voxel.0 as usize]
    }
}

pub fn sdf_chunk_map_builder(chunk_shape: Point3i) -> SdfChunkMapBuilder {
    SdfChunkMapBuilder::new(chunk_shape, (EMPTY_VOXEL_TYPE, EMPTY_SIGNED_DISTANCE))
}

pub fn empty_compressible_sdf_chunk_map(chunk_shape: Point3i) -> CompressibleSdfChunkMap {
    sdf_chunk_map_builder(chunk_shape).build_with_write_storage(
        FastCompressibleChunkStorageNx2::with_bytes_compression(Lz4 { level: 10 }),
    )
}

pub fn empty_sdf_chunk_hash_map(chunk_shape: Point3i) -> SdfChunkHashMap {
    sdf_chunk_map_builder(chunk_shape).build_with_hash_map_storage()
}

pub fn ambient_sdf_array(extent: Extent3i) -> SdfArray {
    SdfArray::fill(extent, (VoxelType(0), Sd8::ONE))
}

pub type SdfArray = Array3x2<VoxelType, Sd8>;

pub type SdfChunkMapBuilder = ChunkMapBuilder3x2<VoxelType, Sd8>;
pub type SdfChunkHashMap = ChunkHashMap3x2<VoxelType, Sd8>;

pub type SdfChunkCache = LocalChunkCache3<SdfArray>;
pub type ThreadLocalVoxelCache = ThreadLocalResource<SdfChunkCache>;

pub type CompressibleSdfChunkMap = CompressibleChunkMap3x2<Lz4, VoxelType, Sd8>;
pub type CompressibleSdfChunkMapReader<'a> = CompressibleChunkMapReader3x2<'a, Lz4, VoxelType, Sd8>;
