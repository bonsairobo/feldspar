use crate::prelude::{witness_superchunk_extent, Config, FrameMapChanges, VoxelDb, Witness};

use bevy::prelude::*;
use bevy::tasks::ComputeTaskPool;
use building_blocks::prelude::*;
use std::collections::HashSet;
use std::iter::FromIterator;

pub fn chunk_loader_system(
    config: Res<Config>,
    db: Res<VoxelDb>,
    mut change_buffer: ResMut<FrameMapChanges>,
    mut witnesses: Query<(&mut Witness, &Transform)>,
    pool: Res<ComputeTaskPool>,
) {
    for (mut witness, tfm) in witnesses.iter_mut() {
        let center = Point3f::from(tfm.translation).in_voxel();
        let witness_extent = witness_superchunk_extent(
            center,
            config.witness_radius,
            config.map.superchunk_exponent,
        );

        let prev_transform = witness.previous_transform;
        witness.previous_transform = Some(*tfm);

        let prev_superchunks = if let Some(prev_transform) = prev_transform {
            let prev_center = Point3f::from(prev_transform.translation).in_voxel();
            let prev_witness_extent = witness_superchunk_extent(
                prev_center,
                config.witness_radius,
                config.map.superchunk_exponent,
            );

            if prev_witness_extent == witness_extent {
                continue;
            }

            HashSet::<Point3i>::from_iter(prev_witness_extent.0.iter_points())
        } else {
            // This is the first frame for this witness, so we need to spawn all superchunks in range.
            HashSet::new()
        };

        // PERF: this could certainly be more efficient with sorted vecs or something
        let superchunks = HashSet::<Point3i>::from_iter(witness_extent.0.iter_points());
        let new_superchunks = &superchunks - &prev_superchunks;
        let old_superchunks = &prev_superchunks - &superchunks;

        log::debug!(
            "Chunk loader: removing {}, inserting {}",
            old_superchunks.len(),
            new_superchunks.len()
        );

        for new_superchunk in new_superchunks.into_iter() {
            let octant = Octant::new(config.map.superchunk_exponent as i32, new_superchunk);
            pool.scope(|s| {
                s.spawn(db.load_superchunk_into_change_buffer(octant, &mut change_buffer))
            });
        }

        for old_superchunk in old_superchunks.into_iter() {
            let octant = Octant::new(config.map.superchunk_exponent as i32, old_superchunk);
            change_buffer.mark_superchunk_for_eviction(octant);
        }
    }
}
