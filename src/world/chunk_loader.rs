use crate::{Config, VoxelEditor, VoxelWorldDb, Witness};

use bevy::prelude::*;
use bevy::tasks::ComputeTaskPool;
use building_blocks::prelude::*;
use std::collections::HashSet;
use std::iter::FromIterator;

pub fn chunk_loader_system(
    config: Res<Config>,
    db: Res<VoxelWorldDb>,
    mut editor: VoxelEditor,
    mut witnesses: Query<(&mut Witness, &Transform)>,
    pool: Res<ComputeTaskPool>,
) {
    for (mut witness, tfm) in witnesses.iter_mut() {
        let center = Point3f::from(tfm.translation).in_voxel();
        let prev_center = witness
            .previous_transform
            .map(|t| Point3f::from(t.translation).in_voxel())
            .unwrap_or(center);

        let prev_witness_extent = witness_superchunk_extent(
            prev_center,
            config.witness_radius,
            config.map.superchunk_exponent,
        );
        let witness_extent = witness_superchunk_extent(
            center,
            config.witness_radius,
            config.map.superchunk_exponent,
        );

        // PERF: this could certainly be more efficient with sorted vecs or something
        let prev_superchunks = HashSet::<Point3i>::from_iter(prev_witness_extent.0.iter_points());
        let superchunks = HashSet::<Point3i>::from_iter(witness_extent.0.iter_points());
        let new_superchunks = &superchunks - &prev_superchunks;
        let old_superchunks = &prev_superchunks - &superchunks;

        for new_superchunk in new_superchunks.into_iter() {
            let octant = Octant::new(config.map.superchunk_exponent as i32, new_superchunk);
            pool.scope(|s| s.spawn(db.load_octant_into_map(0, octant, &mut editor)));
        }

        // for old_superchunk in old_superchunks.into_iter() {
        //     editor.map.storage_mut().remove();
        // }

        witness.previous_transform = Some(*tfm);
    }
}

fn witness_superchunk_extent(
    center: Point3i,
    radius: i32,
    superchunk_exponent: u8,
) -> ChunkUnits<Extent3i> {
    let witness_min = center - Point3i::fill(radius);
    let witness_max = center + Point3i::fill(radius);
    let witness_extent =
        Extent3i::from_min_and_shape(witness_min, witness_max) >> superchunk_exponent;

    ChunkUnits(witness_extent)
}
