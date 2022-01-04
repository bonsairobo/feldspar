use crate::prelude::SdfVoxelMap;

use bevy::prelude::*;

/// A system that flushes thread-local voxel chunk caches into the map's main cache.
pub fn chunk_cache_flusher_system(mut voxel_map: ResMut<SdfVoxelMap>) {
    voxel_map.voxels.storage_mut().flush_thread_local_caches();
}
