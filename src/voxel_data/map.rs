use crate::{
    ThreadLocalResource, ThreadLocalResourceHandle, VoxelMaterial, VoxelType, VoxelTypeInfo,
    EMPTY_SIGNED_DISTANCE,
};

use building_blocks::{prelude::*, storage::FastArrayCompressionNx2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
pub struct MapConfig {
    pub superchunk_exponent: u8,
    pub chunk_exponent: u8,
    pub num_lods: u8,
}

impl Default for MapConfig {
    fn default() -> Self {
        Self {
            superchunk_exponent: 9,
            chunk_exponent: 4,
            num_lods: 4,
        }
    }
}

impl MapConfig {
    pub fn chunk_shape(&self) -> Point3i {
        Point3i::fill(1 << self.chunk_exponent)
    }
}

pub struct SdfVoxelMap {
    pub chunk_index: OctreeChunkIndex,
    pub voxels: CompressibleSdfChunkMap,
    pub palette: SdfVoxelPalette,
}

impl SdfVoxelMap {
    pub fn new_empty(config: MapConfig) -> Self {
        let MapConfig {
            superchunk_exponent,
            chunk_exponent,
            num_lods,
        } = config;

        let chunk_shape = Point3i::fill(1 << chunk_exponent);

        Self {
            chunk_index: OctreeChunkIndex::new_empty(superchunk_exponent, chunk_exponent, num_lods),
            voxels: empty_compressible_sdf_chunk_map(chunk_shape),
            palette: SdfVoxelPalette::new(vec![
                VoxelTypeInfo {
                    is_empty: true,
                    material: VoxelMaterial::NULL,
                },
                VoxelTypeInfo {
                    is_empty: false,
                    material: VoxelMaterial(0),
                },
                VoxelTypeInfo {
                    is_empty: false,
                    material: VoxelMaterial(1),
                },
                VoxelTypeInfo {
                    is_empty: false,
                    material: VoxelMaterial(2),
                },
                VoxelTypeInfo {
                    is_empty: false,
                    material: VoxelMaterial(3),
                },
            ]),
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

    pub fn unload_superchunk(&mut self, octant: Octant, mut chunk_key_rx: impl FnMut(ChunkKey3)) {
        if let Some(octree) = self.chunk_index.pop_superchunk(octant.minimum()) {
            octree.visit_all_points(|chunk_p| {
                let chunk_min = chunk_p << self.chunk_index.chunk_exponent();
                let key = ChunkKey::new(0, chunk_min);
                self.voxels
                    .storage_mut()
                    .remove(ChunkKey::new(0, chunk_min));
                chunk_key_rx(key);
            });
        }
    }
}

#[derive(Clone, Default)]
pub struct SdfVoxelPalette {
    infos: Vec<VoxelTypeInfo>,
}

impl SdfVoxelPalette {
    pub fn new(infos: Vec<VoxelTypeInfo>) -> Self {
        Self { infos }
    }

    pub fn get_voxel_type_info(&self, voxel: VoxelType) -> &VoxelTypeInfo {
        &self.infos[voxel.0 as usize]
    }
}

pub fn sdf_chunk_map_builder(chunk_shape: Point3i) -> SdfChunkMapBuilder {
    SdfChunkMapBuilder::new(chunk_shape, (VoxelType::EMPTY, EMPTY_SIGNED_DISTANCE))
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

pub type SdfArrayCompression = FastArrayCompressionNx2<[i32; 3], Lz4, VoxelType, Sd8>;

pub type SdfChunkMapBuilder = ChunkMapBuilder3x2<VoxelType, Sd8>;
pub type SdfChunkHashMap = ChunkHashMap3x2<VoxelType, Sd8>;

pub type SdfChunkCache = LocalChunkCache3<SdfArray>;
pub type ThreadLocalVoxelCache = ThreadLocalResource<SdfChunkCache>;

pub type CompressibleSdfChunkMap = CompressibleChunkMap3x2<Lz4, VoxelType, Sd8>;
pub type CompressibleSdfChunkMapReader<'a> = CompressibleChunkMapReader3x2<'a, Lz4, VoxelType, Sd8>;
