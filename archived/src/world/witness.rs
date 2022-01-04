use building_blocks::prelude::*;

use bevy::transform::components::Transform;

/// A component for any entity that needs to witness the voxel map, i.e. chunks should be loaded around it, and detail should be
/// highest near it. This only has any effect if the entity also has a `Transform`.
#[derive(Default)]
pub struct Witness {
    pub(crate) previous_transform: Option<Transform>,
}

pub fn witness_superchunk_extent(
    center: Point3i,
    radius: i32,
    superchunk_exponent: u8,
) -> ChunkUnits<Extent3i> {
    let witness_min = center - Point3i::fill(radius);
    let witness_max = center + Point3i::fill(radius);
    let witness_extent = Extent3i::from_min_and_max(
        witness_min >> superchunk_exponent,
        witness_max >> superchunk_exponent,
    );

    ChunkUnits(witness_extent)
}
