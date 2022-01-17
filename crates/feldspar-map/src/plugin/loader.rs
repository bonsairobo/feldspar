use super::config::MapConfig;
use super::Witness;
use crate::chunk::CompressedChunk;
use crate::clipmap::{ChunkClipMap, NodeKey, NodePtr};
use crate::database::{ArchivedChangeIVec, MapDb};
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
    /// The maximum number of pending load tasks.
    pub max_pending_load_tasks: usize,
}

impl Default for LoaderConfig {
    fn default() -> Self {
        Self {
            load_batch_size: 256,
            max_pending_load_tasks: 16,
        }
    }
}

pub struct LoadedBatch {
    reads: Vec<LoadedChunk>,
}

pub struct LoadedChunk {
    key: NodeKey<IVec3>,
    archived_chunk: Option<ArchivedChangeIVec<CompressedChunk>>,
    nearest_ancestor_ptr: Option<NodePtr>,
}

pub struct PendingLoadTasks {
    tasks: VecDeque<Task<LoadedBatch>>,
}

pub fn loader_system(
    config: Res<MapConfig>,
    witness_transforms: Query<(&Witness, &Transform)>,
    io_pool: Res<IoTaskPool>,
    db: Res<Arc<MapDb>>, // PERF: better option than Arc?
    mut clipmap: ResMut<ChunkClipMap>,
    mut load_tasks: ResMut<PendingLoadTasks>,
) {
    let PendingLoadTasks { tasks } = &mut *load_tasks;

    // Complete pending load tasks in queue order.
    // PERF: is this the best way to poll a sequence of futures?
    while let Some(mut task) = tasks.pop_front() {
        if let Some(loaded_batch) = future::block_on(future::poll_once(&mut task)) {
            // Insert the chunks into the clipmap and mark the nodes as loaded.
            for LoadedChunk {
                key,
                archived_chunk,
                nearest_ancestor_ptr,
            } in loaded_batch.reads.into_iter()
            {
                clipmap.complete_pending_load(
                    key,
                    nearest_ancestor_ptr,
                    // PERF: maybe just decompress directly from the archived bytes here?
                    archived_chunk.map(|c| c.deserialize().unwrap_insert()),
                )
            }
        } else {
            tasks.push_front(task);
            break;
        }
    }

    // PERF: this does a bunch of redundant work when the clip spheres of multiple witnesses overlap
    for (witness, tfm) in witness_transforms.iter() {
        if let Some(prev_tfm) = witness.previous_transform.as_ref() {
            // TODO: use .as_vec3a()
            let old_witness_pos = VoxelUnits(Vec3A::from(prev_tfm.translation.to_array()));
            let new_witness_pos = VoxelUnits(Vec3A::from(tfm.translation.to_array()));

            // Insert loading sentinel nodes to mark trees for async loading.
            clipmap.broad_phase_load_search(old_witness_pos, new_witness_pos);

            if tasks.len() >= config.loader.max_pending_load_tasks {
                continue;
            }

            // Find a batch of nodes to load.
            let search = clipmap.near_phase_load_search(new_witness_pos);
            let batch_keys: Vec<_> = search.take(config.loader.load_batch_size).collect();

            // Spawn a new task to load those nodes.
            let db_clone = db.clone();
            let load_task = io_pool.spawn(async move {
                // PERF: Should this batch be a single task?
                LoadedBatch {
                    reads: batch_keys
                        .into_iter()
                        .map(move |(key, nearest_ancestor_ptr)| LoadedChunk {
                            key,
                            archived_chunk: db_clone.read_working_version(key.into()).unwrap(),
                            nearest_ancestor_ptr,
                        })
                        .collect(),
                }
            });
            tasks.push_back(load_task);
        }
    }
}
