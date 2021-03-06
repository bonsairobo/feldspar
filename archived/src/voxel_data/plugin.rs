use super::{
    chunk_cache_flusher::chunk_cache_flusher_system,
    chunk_compressor::chunk_compressor_system,
    empty_chunk_remover::empty_chunk_remover_system,
    map_changes::{double_buffering_system, DirtyChunks, FrameMapChanges},
    EmptyChunks, SdfVoxelMap,
};
use crate::prelude::MapConfig;

use bevy::{app::prelude::*, ecs::prelude::*};

pub use super::chunk_compressor::ChunkCacheConfig;

/// This plugin manages the `SdfVoxelMap` resource, which contains all of the voxel data in the current world.
///
/// Thread-local caches are used for voxel chunks that are decompressed during access. At the end of the frame, these caches are
/// flushed back into the `SdfVoxelMap`'s global cache.
///
/// If the size of the global chunk cache grows beyond a limit, one of the plugin systems will start compressing the
/// least-recently-used chunks to save space.
///
/// In order to get maximum read parallelism from the voxel map, use the `VoxelEditor`, a `SystemParam` that writes your edits
/// out of place. The edits will get merged into the `SdfVoxelMap` at the end of the same frame. The edited chunks will also be
/// marked as "dirty" in the `DirtyChunks` resource, which makes it easier to do post-processing when chunks change.
///
/// **WARNING**: Cached reads will always be flushed before double-buffered writes. This means if you try to write directly into
/// the `SdfVoxelMap`, you risk having your changes overwritten by the flush.
pub struct VoxelDataPlugin {
    map_config: MapConfig,
    cache_config: ChunkCacheConfig,
}

impl VoxelDataPlugin {
    pub fn new(map_config: MapConfig, cache_config: ChunkCacheConfig) -> Self {
        Self {
            map_config,
            cache_config,
        }
    }
}

impl Plugin for VoxelDataPlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.insert_resource(self.cache_config)
            .insert_resource(SdfVoxelMap::new_empty(self.map_config))
            .insert_resource(FrameMapChanges::new(self.map_config.chunk_shape()))
            .insert_resource(DirtyChunks::default())
            .insert_resource(EmptyChunks::default())
            // Ordering the cache flusher and double buffering is important, because we don't want to overwrite edits with
            // locally cached chunks. Similarly, empty chunks should be removed before new edits are merged in.
            .add_system_set_to_stage(
                CoreStage::Last,
                SystemSet::new()
                    .before("merge_changes")
                    .with_system(chunk_cache_flusher_system.system())
                    .with_system(empty_chunk_remover_system.system()),
            )
            .add_system_to_stage(
                CoreStage::Last,
                double_buffering_system.system().label("merge_changes"),
            )
            .add_system_to_stage(CoreStage::Last, chunk_compressor_system.system());
    }
}
