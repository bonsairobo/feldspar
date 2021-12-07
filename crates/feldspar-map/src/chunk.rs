use crate::{NdView, PaletteId8, Sd8};

use bytemuck::{bytes_of_mut, cast_slice, Pod, Zeroable};
use ilattice::glam::{const_ivec3, IVec3};
use lz4_flex::frame::{FrameDecoder, FrameEncoder};
use ndshape::{ConstPow2Shape3i32, ConstShape};
use static_assertions::const_assert_eq;
use std::io;
use std::mem;

/// The standard 3D array shape for chunks.
pub type ChunkShape = ConstPow2Shape3i32<4, 4, 4>;
const_assert_eq!(ChunkShape::SIZE, 16 * 16 * 16);
pub const CHUNK_SIZE: usize = ChunkShape::SIZE as usize;
pub const CHUNK_SHAPE_IVEC3: IVec3 = const_ivec3!([16; 3]);
pub const CHUNK_SHAPE_LOG2_IVEC3: IVec3 = const_ivec3!([4; 3]);

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
        let mut sdf_reader = cast_slice(self.sdf.as_ref());
        io::copy(&mut sdf_reader, &mut encoder).unwrap() as usize;
        let mut palette_reader = cast_slice(self.palette_ids.as_ref());
        io::copy(&mut palette_reader, &mut encoder).unwrap();
        CompressedChunk {
            bytes: encoder.finish().unwrap().into_boxed_slice(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompressedChunk {
    pub bytes: Box<[u8]>,
}

const_assert_eq!(
    mem::size_of::<CompressedChunk>(),
    2 * mem::size_of::<usize>()
);

impl CompressedChunk {
    pub fn decompress(&self) -> Chunk {
        let mut chunk = Chunk {
            sdf: [Sd8(0); CHUNK_SIZE],
            palette_ids: [0; CHUNK_SIZE],
        };
        let mut decoder = FrameDecoder::new(self.bytes.as_ref());
        let mut writer = bytes_of_mut(&mut chunk);
        io::copy(&mut decoder, &mut writer).unwrap();
        chunk
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

    #[test]
    fn chunk_compression_roundtrip() {
        let chunk = Chunk::default();
        assert_eq!(chunk.compress().decompress(), chunk);
    }
}
