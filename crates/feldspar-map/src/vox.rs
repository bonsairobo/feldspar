use crate::core::SmallKeyHashMap;
use crate::core::glam::IVec3;
use crate::chunk::Chunk;
use crate::coordinates::*;
use crate::sdf::Sd8;
use crate::units::*;

use vox_format::types::{ColorIndex, Model, Voxel};

pub fn convert_vox_model_to_chunks(model: &Model) -> SmallKeyHashMap<ChunkUnits<IVec3>, Chunk> {
    let mut chunks = SmallKeyHashMap::default();
    for Voxel { point: p, color_index: ColorIndex(palette_id) } in model.voxels.iter() {
        let p = IVec3::new(p.x.into(), p.y.into(), p.z.into());
        let chunk_coords = in_chunk(VoxelUnits(p));
        let chunk = chunks.entry(chunk_coords).or_insert_with(Chunk::default);
        let VoxelUnits(chunk_min) = chunk_min(chunk_coords);
        chunk.set_voxel(p - chunk_min, *palette_id, Sd8::MAX);
    }
    chunks
}
