use super::ThreadLocalVoxelCache;

use crate::SdfVoxelMap;

use bevy::prelude::*;

/// A system that flushes thread-local voxel chunk caches into the global map's cache.
pub fn chunk_cache_flusher_system(
    mut local_caches: ResMut<ThreadLocalVoxelCache>,
    mut voxel_map: ResMut<SdfVoxelMap>,
) {
    let taken_caches = std::mem::replace(&mut *local_caches, ThreadLocalVoxelCache::new());
    for cache in taken_caches.into_iter() {
        voxel_map.voxels.storage_mut().flush_local_cache(cache);
    }
}
