use super::config::MapConfig;
use super::Witness;
use crate::clipmap::{new_nodes_intersecting_sphere, ChunkClipMap};
use crate::units::VoxelUnits;

use feldspar_core::glam::Vec3A;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize, Serialize)]
pub struct LoaderConfig {
    /// The number of chunks to start loading in a single frame (batch).
    pub load_batch_size: usize,
    /// The maximum number of outstanding load tasks.
    pub max_outstanding_load_tasks: usize,
}

impl Default for LoaderConfig {
    fn default() -> Self {
        Self {
            load_batch_size: 256,
            max_outstanding_load_tasks: 16,
        }
    }
}

pub struct LoadBatch {}

pub struct OutstandingLoadTasks {
    tasks: Vec<Task<LoadBatch>>,
}

pub fn loader_system(
    config: Res<MapConfig>,
    witness_transforms: Query<(&Witness, &Transform)>,
    io_pool: Res<IoTaskPool>,
    mut clipmap: ResMut<ChunkClipMap>,
    mut load_tasks: ResMut<OutstandingLoadTasks>,
) {
    // PERF: this does a bunch of redundant work when the clip spheres of multiple witnesses overlap
    for (witness, tfm) in witness_transforms.iter() {
        if let Some(prev_tfm) = witness.previous_transform.as_ref() {
            let old_witness_pos = VoxelUnits(Vec3A::from(prev_tfm.translation.to_array()));
            let new_witness_pos = VoxelUnits(Vec3A::from(tfm.translation.to_array()));
            new_nodes_intersecting_sphere(
                config.streaming,
                clipmap.octree.root_level(),
                old_witness_pos,
                new_witness_pos,
                |node_slot| clipmap.insert_loading_node(node_slot.node_key()),
            )
        }
    }
}
