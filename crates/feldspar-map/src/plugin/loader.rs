use super::config::MapConfig;
use super::Witness;
use crate::chunk::CompressedChunk;
use crate::clipmap::{new_nodes_intersecting_sphere, ChunkClipMap};
use crate::database::{ArchivedChangeIVec, ChunkDbKey, MapDb};
use crate::units::VoxelUnits;

use feldspar_core::glam::Vec3A;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
use futures_lite::future;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;

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

pub struct LoadedBatch {
    reads: Vec<(ChunkDbKey, Option<ArchivedChangeIVec<CompressedChunk>>)>,
}

pub struct OutstandingLoadTasks {
    tasks: VecDeque<Task<LoadedBatch>>,
}

impl OutstandingLoadTasks {
    pub fn num_tasks(&self) -> usize {
        self.tasks.len()
    }

    pub fn add_task(&mut self, task: Task<LoadedBatch>) {
        self.tasks.push_back(task);
    }
}

pub fn loader_system(
    config: Res<MapConfig>,
    witness_transforms: Query<(&Witness, &Transform)>,
    io_pool: Res<IoTaskPool>,
    db: Res<Arc<MapDb>>, // PERF: better option than Arc?
    mut clipmap: ResMut<ChunkClipMap>,
    mut load_tasks: ResMut<OutstandingLoadTasks>,
) {
    // Complete outstanding load tasks in queue order.
    // PERF: is this the best way to poll a sequence of futures?
    while let Some(mut task) = load_tasks.tasks.pop_front() {
        if let Some(loaded_batch) = future::block_on(future::poll_once(&mut task)) {
            // Insert the chunks into the clipmap and mark the nodes as loaded.
            todo!()
        } else {
            load_tasks.tasks.push_front(task);
        }
    }

    // PERF: this does a bunch of redundant work when the clip spheres of multiple witnesses overlap
    for (witness, tfm) in witness_transforms.iter() {
        if let Some(prev_tfm) = witness.previous_transform.as_ref() {
            // TODO: use .as_vec3a()
            let old_witness_pos = VoxelUnits(Vec3A::from(prev_tfm.translation.to_array()));
            let new_witness_pos = VoxelUnits(Vec3A::from(tfm.translation.to_array()));

            // Insert loading sentinel nodes to mark trees for async loading.
            new_nodes_intersecting_sphere(
                config.streaming,
                clipmap.octree.root_level(),
                old_witness_pos,
                new_witness_pos,
                |node_slot| clipmap.insert_loading_node(node_slot.node_key()),
            );

            if load_tasks.num_tasks() >= config.loader.max_outstanding_load_tasks {
                continue;
            }

            // Find a batch of nodes to load.
            let mut batch_keys = Vec::new();
            clipmap.loading_nodes(
                config.loader.load_batch_size,
                new_witness_pos,
                |level, coords| {
                    batch_keys.push(ChunkDbKey::new(level, coords.into_inner().into()));
                },
            );

            // Spawn a new task to load those nodes.
            let db_clone = db.clone();
            let load_task = io_pool.spawn(async move {
                // PERF: Should this batch be a single task?
                LoadedBatch {
                    reads: batch_keys
                        .into_iter()
                        .map(move |key| (key, db_clone.read_working_version(key).unwrap()))
                        .collect(),
                }
            });
            load_tasks.tasks.push_back(load_task);
        }
    }
}
