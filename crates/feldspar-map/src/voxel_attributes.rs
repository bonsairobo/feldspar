/// The data stored for each *type* of voxel, i.e. inside of a [`Palette8`](crate::palette::Palette8) for each
/// [`PaletteId8`](crate::palette::PaletteId8).
pub struct VoxelAttributes {
    pub is_collidable: bool,
    pub material_id: MaterialId,
}

pub struct MaterialId(pub u8);
