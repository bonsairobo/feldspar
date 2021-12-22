use crate::sampling::OctantKernel;
use crate::{coordinates::min_child_coords, ndview::NdView, palette::PaletteId8, sdf::Sd8};

use bytemuck::{bytes_of, bytes_of_mut, Pod, Zeroable};
use ilattice::glam::{const_ivec3, const_vec3a, IVec3, Vec3A};
use lz4_flex::frame::{FrameDecoder, FrameEncoder};
use ndshape::{ConstPow2Shape3i32, ConstShape};
use rkyv::{Archive, Deserialize, Serialize};
use static_assertions::const_assert_eq;
use std::io;
use std::mem;

/// The standard 3D array shape for chunks.
pub type ChunkShape = ConstPow2Shape3i32<4, 4, 4>;
const_assert_eq!(ChunkShape::SIZE, 16 * 16 * 16);
pub const CHUNK_SIZE: usize = ChunkShape::SIZE as usize;
pub const CHUNK_SHAPE_IVEC3: IVec3 = const_ivec3!([16; 3]);
pub const CHUNK_SHAPE_VEC3A: Vec3A = const_vec3a!([16.0; 3]);
pub const CHUNK_SHAPE_LOG2_IVEC3: IVec3 = const_ivec3!([4; 3]);
pub const HALF_CHUNK_SHAPE_LOG2_IVEC3: IVec3 = const_ivec3!([3; 3]);
pub const HALF_CHUNK_EDGE_LENGTH: i32 = 8;

/// "As far *outside* of the terrain surface as possible."
pub const AMBIENT_SD8: Sd8 = Sd8::MAX;

/// The fundamental unit of voxel storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Chunk {
    /// Signed distance field for geometry.
    pub sdf: SdfChunk,
    /// Voxel "materials" that map into attributes of some [`Palette8`](crate::Palette8).
    pub palette_ids: PaletteIdChunk,
}

unsafe impl Zeroable for Chunk {}
unsafe impl Pod for Chunk {}

impl Default for Chunk {
    fn default() -> Self {
        Self {
            sdf: [AMBIENT_SD8; CHUNK_SIZE],
            palette_ids: [0; CHUNK_SIZE],
        }
    }
}

const_assert_eq!(mem::size_of::<Chunk>(), 8192);

pub type SdfChunk = [Sd8; CHUNK_SIZE];
pub type PaletteIdChunk = [PaletteId8; CHUNK_SIZE];

const_assert_eq!(mem::size_of::<SdfChunk>(), 4096);
const_assert_eq!(mem::size_of::<PaletteIdChunk>(), 4096);

impl Chunk {
    pub fn sdf_view(&self) -> NdView<Sd8, &SdfChunk, ChunkShape> {
        NdView::new(&self.sdf, ChunkShape {})
    }

    pub fn sdf_view_mut(&mut self) -> NdView<Sd8, &mut SdfChunk, ChunkShape> {
        NdView::new(&mut self.sdf, ChunkShape {})
    }

    pub fn palette_view(&self) -> NdView<PaletteId8, &PaletteIdChunk, ChunkShape> {
        NdView::new(&self.palette_ids, ChunkShape {})
    }

    pub fn palette_view_mut(&mut self) -> NdView<PaletteId8, &mut PaletteIdChunk, ChunkShape> {
        NdView::new(&mut self.palette_ids, ChunkShape {})
    }

    pub fn compress(&self) -> CompressedChunk {
        let mut encoder = FrameEncoder::new(Vec::new());
        let mut reader = bytes_of(self);
        io::copy(&mut reader, &mut encoder).unwrap();
        CompressedChunk {
            bytes: encoder.finish().unwrap().into_boxed_slice(),
        }
    }

    pub fn from_compressed_bytes(bytes: &[u8]) -> Chunk {
        let mut chunk = Chunk {
            sdf: [Sd8(0); CHUNK_SIZE],
            palette_ids: [0; CHUNK_SIZE],
        };
        let mut decoder = FrameDecoder::new(bytes);
        let mut writer = bytes_of_mut(&mut chunk);
        io::copy(&mut decoder, &mut writer).unwrap();
        chunk
    }

    /// Downsamples the SDF and palette IDs from `self` at half resolution into one octant of a parent chunk.
    pub fn downsample_into(
        &self,
        kernel: &mut OctantKernel,
        self_coords: IVec3,
        parent_coords: IVec3,
        parent_chunk: &mut Chunk,
    ) {
        let min_child = min_child_coords(parent_coords);
        let child_offset = self_coords - min_child;
        let dst_offset =
            ChunkShape::linearize((child_offset << HALF_CHUNK_SHAPE_LOG2_IVEC3).to_array())
                as usize;

        // SDF is downsampled as a mean of the 8 children.
        kernel.downsample_sdf(&self.sdf, dst_offset, &mut parent_chunk.sdf);

        // Palette IDs are downsampled as the mode of the 8 children.
        kernel.downsample_labels(&self.palette_ids, dst_offset, &mut parent_chunk.palette_ids);
    }
}

#[derive(Archive, Clone, Deserialize, Debug, Eq, PartialEq, Serialize)]
pub struct CompressedChunk {
    pub bytes: Box<[u8]>,
}

const_assert_eq!(
    mem::size_of::<CompressedChunk>(),
    2 * mem::size_of::<usize>()
);

impl CompressedChunk {
    pub fn decompress(&self) -> Chunk {
        Chunk::from_compressed_bytes(&self.bytes)
    }
}

// ████████╗███████╗███████╗████████╗
// ╚══██╔══╝██╔════╝██╔════╝╚══██╔══╝
//    ██║   █████╗  ███████╗   ██║
//    ██║   ██╔══╝  ╚════██║   ██║
//    ██║   ███████╗███████║   ██║
//    ╚═╝   ╚══════╝╚══════╝   ╚═╝

#[cfg(test)]
mod test {
    use super::*;
    use crate::{coordinates::chunk_extent_from_min_ivec3, units::VoxelUnits};

    #[test]
    fn compress_default_chunk() {
        let chunk = Chunk::default();
        let compressed = chunk.compress();
        let compression_ratio = compressed.bytes.len() as f32 / (mem::size_of::<Chunk>() as f32);
        assert!(compression_ratio < 0.008, "{}", compression_ratio);
        assert_eq!(compressed.decompress(), chunk);
    }

    #[test]
    fn compress_chunk_with_sphere_sdf() {
        let mut chunk = Chunk::default();
        let VoxelUnits(extent) = chunk_extent_from_min_ivec3(VoxelUnits(IVec3::ZERO));
        let center = (extent.minimum + extent.least_upper_bound()) / 2;
        for p in extent.iter3() {
            let d = p.as_vec3a().distance(center.as_vec3a());
            let i = ChunkShape::linearize(p.to_array()) as usize;
            chunk.sdf[i] = (d - 8.0).into();
            if d < 8.0 {
                chunk.palette_ids[i] = 1;
            }
        }

        let compressed = chunk.compress();
        let compression_ratio = compressed.bytes.len() as f32 / (mem::size_of::<Chunk>() as f32);
        assert!(compression_ratio < 0.19, "{}", compression_ratio);
        assert_eq!(compressed.decompress(), chunk);
    }
}
