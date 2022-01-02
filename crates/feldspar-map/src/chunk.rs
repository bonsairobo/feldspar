use crate::core::geometry::Ray;
use crate::core::glam::{const_ivec3, const_vec3a, IVec3, Vec3A};
use crate::core::rkyv::{Archive, Deserialize, Serialize};
use crate::core::static_assertions::const_assert_eq;
use crate::sampling::OctantKernel;
use crate::{coordinates::*, ndview::NdView, palette::PaletteId8, sdf::Sd8, units::*};

use bytemuck::{bytes_of, bytes_of_mut, Pod, Zeroable};
use grid_ray::GridRayIter3;
use lz4_flex::frame::{FrameDecoder, FrameEncoder};
use ndshape::{ConstPow2Shape3i32, ConstShape, ConstShape3i32};
use std::io;
use std::mem;

/// The standard 3D array shape for chunks.
pub type ChunkShape = ConstPow2Shape3i32<4, 4, 4>;
const_assert_eq!(ChunkShape::SIZE, 16 * 16 * 16);
pub const CHUNK_SIZE: usize = ChunkShape::SIZE as usize;
pub const CHUNK_SHAPE_IVEC3: IVec3 = const_ivec3!(ChunkShape::ARRAY);
pub const CHUNK_SHAPE_VEC3A: Vec3A = const_vec3a!([16.0; 3]);
pub const CHUNK_SHAPE_LOG2_IVEC3: IVec3 = const_ivec3!([4; 3]);
pub const HALF_CHUNK_SHAPE_LOG2_IVEC3: IVec3 = const_ivec3!([3; 3]);
pub const HALF_CHUNK_EDGE_LENGTH: i32 = 8;

/// The shape (in voxels) of a padded chunk, i.e. the full set of voxels necessary to produce a chunk mesh.
pub type PaddedChunkShape = ConstShape3i32<18, 18, 18>;
/// [`IVec3`] version of [`PaddedChunkShape`].
pub const PADDED_CHUNK_SHAPE_IVEC3: IVec3 = const_ivec3!(PaddedChunkShape::ARRAY);

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

    pub fn set_voxel(&mut self, offset: IVec3, palette_id: PaletteId8, sdf: Sd8) {
        let index = ChunkShape::linearize(offset.to_array()) as usize;
        self.sdf[index] = sdf;
        self.palette_ids[index] = palette_id;
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

    /// Visit every voxel in `chunk` that intersects the ray. Return `false` to stop the traversal.
    pub fn ray_intersections(
        &self,
        chunk_coords: ChunkUnits<IVec3>,
        ray: &Ray,
        mut visitor: impl FnMut(f32, IVec3, Sd8, PaletteId8) -> bool,
    ) {
        let VoxelUnits(chunk_aabb) = chunk_extent_vec3a(chunk_coords);
        if let Some([t_enter_chunk, t_exit_chunk]) = ray.cast_at_extent(chunk_aabb) {
            // Nudge the start and end a little bit to be sure we stay in the chunk.
            let duration_inside_chunk = t_exit_chunk - t_enter_chunk;
            let nudge_duration = 0.000001 * duration_inside_chunk;
            let t_nudge_start = t_enter_chunk + nudge_duration;
            let nudge_start = ray.position_at(t_nudge_start);

            if !chunk_aabb.contains(nudge_start) {
                return;
            }

            let VoxelUnits(chunk_min) = chunk_min(chunk_coords);
            let nudge_t_max = t_exit_chunk - nudge_duration;
            let iter = GridRayIter3::new(nudge_start, ray.velocity());
            for (t_enter, p) in iter {
                // We technically "advanced the clock" by t_nudge_start before we started this iterator.
                let actual_t_enter = t_enter + t_nudge_start;
                if actual_t_enter > nudge_t_max {
                    break;
                }
                let offset = p - chunk_min;
                let index = ChunkShape::linearize(offset.to_array()) as usize;
                if index >= CHUNK_SIZE {
                    // Floating Point Paranoia: Just avoid panicking from out-of-bounds access at all costs.
                    // TODO: log warning!
                    break;
                }
                if !visitor(actual_t_enter, p, self.sdf[index], self.palette_ids[index]) {
                    break;
                }
            }
        }
    }
}

#[derive(Archive, Clone, Deserialize, Debug, Eq, PartialEq, Serialize)]
#[archive(crate = "crate::core::rkyv")]
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
    use crate::{chunk::AMBIENT_SD8, coordinates::chunk_extent_from_min_ivec3, units::VoxelUnits};

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

    #[test]
    fn ray_intersections_pass_through() {
        let ray = Ray::new(Vec3A::new(-0.5, 0.5, 0.5), Vec3A::new(1.0, 0.0, 0.0));
        let chunk = Chunk::default();
        let chunk_coords = ChunkUnits(IVec3::ZERO);

        let [t_chunk_enter, t_chunk_exit] = ray
            .cast_at_extent(chunk_extent_vec3a(chunk_coords).into_inner())
            .unwrap();

        let mut visited_coords = Vec::new();
        chunk.ray_intersections(chunk_coords, &ray, |t_enter, coords, sdf, palette_id| {
            assert_eq!(sdf, AMBIENT_SD8);
            assert_eq!(palette_id, 0);
            assert!(t_enter >= t_chunk_enter);
            assert!(t_enter < t_chunk_exit);
            visited_coords.push(coords);
            true
        });

        assert_eq!(
            visited_coords.as_slice(),
            &[
                IVec3::new(0, 0, 0),
                IVec3::new(1, 0, 0),
                IVec3::new(2, 0, 0),
                IVec3::new(3, 0, 0),
                IVec3::new(4, 0, 0),
                IVec3::new(5, 0, 0),
                IVec3::new(6, 0, 0),
                IVec3::new(7, 0, 0),
                IVec3::new(8, 0, 0),
                IVec3::new(9, 0, 0),
                IVec3::new(10, 0, 0),
                IVec3::new(11, 0, 0),
                IVec3::new(12, 0, 0),
                IVec3::new(13, 0, 0),
                IVec3::new(14, 0, 0),
                IVec3::new(15, 0, 0),
            ]
        );
    }

    #[test]
    fn ray_intersections_stop_on_voxel() {
        let ray = Ray::new(Vec3A::ONE, Vec3A::new(1.0, 1.0, 1.0));
        let mut chunk = Chunk::default();
        let chunk_coords = ChunkUnits(IVec3::ZERO);
        let VoxelUnits(chunk_min) = chunk_min(chunk_coords);

        // Mark one voxel in the middle to prove that we can hit it and stop.
        chunk.palette_view_mut()[IVec3::new(7, 7, 7) - chunk_min] = 1;

        let mut visited_coords = Vec::new();
        chunk.ray_intersections(chunk_coords, &ray, |_t_enter, coords, sdf, palette_id| {
            assert_eq!(sdf, AMBIENT_SD8);
            visited_coords.push(coords);
            palette_id == 0
        });

        assert_eq!(
            visited_coords.as_slice(),
            &[
                IVec3::new(0, 0, 0),
                IVec3::new(0, 0, 1),
                IVec3::new(0, 1, 1),
                IVec3::new(1, 1, 1),
                IVec3::new(1, 1, 2),
                IVec3::new(1, 2, 2),
                IVec3::new(2, 2, 2),
                IVec3::new(2, 2, 3),
                IVec3::new(2, 3, 3),
                IVec3::new(3, 3, 3),
                IVec3::new(3, 3, 4),
                IVec3::new(3, 4, 4),
                IVec3::new(4, 4, 4),
                IVec3::new(4, 4, 5),
                IVec3::new(4, 5, 5),
                IVec3::new(5, 5, 5),
                IVec3::new(5, 5, 6),
                IVec3::new(5, 6, 6),
                IVec3::new(6, 6, 6),
                IVec3::new(6, 6, 7),
                IVec3::new(6, 7, 7),
                IVec3::new(7, 7, 7),
            ]
        );
    }
}
