use crate::{
    world::witness_superchunk_extent, Config, EditBuffer, SdfVoxelMap, VoxelWorldDb, Witness,
};

use bevy::prelude::*;
use bevy::tasks::ComputeTaskPool;
use building_blocks::prelude::*;
use std::collections::HashSet;
use std::iter::FromIterator;

pub fn chunk_loader_system(
    config: Res<Config>,
    db: Res<VoxelWorldDb>,
    mut map: ResMut<SdfVoxelMap>,
    mut edit_buffer: ResMut<EditBuffer>,
    mut witnesses: Query<(&mut Witness, &Transform)>,
    pool: Res<ComputeTaskPool>,
) {
    for (mut witness, tfm) in witnesses.iter_mut() {
        let center = Point3f::from(tfm.translation).in_voxel();
        let prev_center = witness
            .previous_transform
            .map(|t| Point3f::from(t.translation).in_voxel())
            .unwrap_or(center);

        witness.previous_transform = Some(*tfm);

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

        if prev_witness_extent == witness_extent {
            continue;
        }

        // PERF: this could certainly be more efficient with sorted vecs or something
        let superchunks = HashSet::<Point3i>::from_iter(witness_extent.0.iter_points());
        let prev_superchunks = HashSet::<Point3i>::from_iter(prev_witness_extent.0.iter_points());
        let new_superchunks = &superchunks - &prev_superchunks;
        let old_superchunks = &prev_superchunks - &superchunks;

        for new_superchunk in new_superchunks.into_iter() {
            let octant = Octant::new(config.map.superchunk_exponent as i32, new_superchunk);
            pool.scope(|s| {
                s.spawn(db.load_superchunk_into_map(octant, &mut map, &mut edit_buffer))
            });
        }

        for old_superchunk in old_superchunks.into_iter() {
            let octant = Octant::new(config.map.superchunk_exponent as i32, old_superchunk);
            map.unload_superchunk(octant, &mut edit_buffer);
        }
    }
}
