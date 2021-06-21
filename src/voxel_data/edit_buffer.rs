use crate::{
    empty_sdf_chunk_hash_map, CompressibleSdfChunkMap, CompressibleSdfChunkMapReader, SdfArray,
    SdfChunkHashMap, SdfVoxelMap, VoxelType, EMPTY_SDF_VOXEL,
};

use bevy::prelude::*;
use building_blocks::{prelude::*, storage::SmallKeyHashSet};

/// For the sake of pipelining, all voxels edits are first written out of place here. They can later be merged into another
/// chunk map by overwriting the dirty chunks.
pub struct EditBuffer {
    edited_voxels: SdfChunkHashMap,
    // Includes the edited chunks as well as their neighbors, all of which need to be re-meshed.
    dirty_chunk_mins: SmallKeyHashSet<Point3i>,
}

impl EditBuffer {
    pub fn new(chunk_shape: Point3i) -> Self {
        Self {
            edited_voxels: empty_sdf_chunk_hash_map(chunk_shape),
            dirty_chunk_mins: Default::default(),
        }
    }

    /// This function does read-modify-write of the voxels in `extent`. If a chunk is missing from the backbuffer, it will be
    /// copied from the `reader` before being written.
    ///
    /// If `touch_neighbors`, then all chunks in the Moore Neighborhood of any edited chunk will be marked as dirty. This is
    /// useful when there are dependencies between adjacent chunks that must be considered during post-processing (e.g. during
    /// mesh generation).
    pub fn edit_voxels_out_of_place(
        &mut self,
        reader: &CompressibleSdfChunkMapReader,
        extent: Extent3i,
        edit_func: impl FnMut(Point3i, (&mut VoxelType, &mut Sd8)),
        touch_neighbors: bool,
    ) {
        debug_assert!(reader.chunk_shape().eq(&self.edited_voxels.chunk_shape()));

        // Copy any of the overlapping chunks that don't already exist in the backbuffer, i.e. those chunks which haven't been
        // modified yet.
        for chunk_min in reader.indexer.chunk_mins_for_extent(&extent) {
            let chunk_key = ChunkKey::new(0, chunk_min);
            self.edited_voxels
                .get_mut_chunk_or_insert_with(chunk_key, || {
                    reader
                        .storage()
                        .storage
                        // We don't cache the chunk yet, because we're just going to modify this copy and insert back into the
                        // map later.
                        .copy_without_caching(chunk_key)
                        .map(|c| c.into_decompressed())
                        .unwrap_or_else(|| {
                            SdfArray::fill(
                                reader.indexer.extent_for_chunk_with_min(chunk_min),
                                EMPTY_SDF_VOXEL,
                            )
                        })
                });
        }

        self.dirty_chunks_for_extent(touch_neighbors, extent);

        // Edit the backbuffer.
        self.edited_voxels
            .lod_view_mut(0)
            .for_each_mut(&extent, edit_func);
    }

    pub fn insert_chunk(&mut self, touch_neighbors: bool, chunk_min: Point3i, chunk: SdfArray) {
        // PERF: this could be more efficient if we just took the moore neighborhood in chunk space
        let extent = self
            .edited_voxels
            .indexer
            .extent_for_chunk_with_min(chunk_min);
        self.dirty_chunks_for_extent(touch_neighbors, extent);
        self.edited_voxels
            .write_chunk(ChunkKey::new(0, chunk_min), chunk);
    }

    /// Write all of the edited chunks into `dst_map`. Returns the dirty chunks.
    pub fn merge_edits(self, dst_map: &mut CompressibleSdfChunkMap) -> DirtyChunks {
        let EditBuffer {
            edited_voxels,
            dirty_chunk_mins,
        } = self;

        let chunk_storage = edited_voxels.take_storage();
        let edited_chunk_mins = chunk_storage.chunk_keys().map(|k| k.minimum).collect();

        for (chunk_key, chunk) in chunk_storage.into_iter() {
            dst_map.write_chunk(chunk_key, chunk);
        }

        DirtyChunks {
            edited_chunk_mins,
            dirty_chunk_mins,
        }
    }

    fn dirty_chunks_for_extent(&mut self, touch_neighbors: bool, extent: Extent3i) {
        // Mark the chunks and maybe their neighbors as dirty.
        let dirty_extent = if touch_neighbors {
            let chunk_shape = self.edited_voxels.chunk_shape();

            Extent3i::from_min_and_max(extent.minimum - chunk_shape, extent.max() + chunk_shape)
        } else {
            extent
        };
        for chunk_key in self
            .edited_voxels
            .indexer
            .chunk_mins_for_extent(&dirty_extent)
        {
            self.dirty_chunk_mins.insert(chunk_key);
        }
    }
}

/// The sets of chunk keys that have either been edited directly or marked as dirty, by virtue of neighboring an edited chunk.
#[derive(Default)]
pub struct DirtyChunks {
    pub edited_chunk_mins: Vec<Point3i>,
    pub dirty_chunk_mins: SmallKeyHashSet<Point3i>,
}

/// Merges edits from the `EditBuffer` into the `SdfVoxelMap`. By setting the `DirtyChunks` resource, the
/// `chunk_processor_system` will be notified to process dirty chunks on the next frame.
pub fn double_buffering_system(
    mut voxel_map: ResMut<SdfVoxelMap>,
    mut edit_buffer: ResMut<EditBuffer>,
    mut dirty_chunks: ResMut<DirtyChunks>,
) {
    let edit_buffer = std::mem::replace(
        &mut *edit_buffer,
        EditBuffer::new(voxel_map.voxels.chunk_shape()),
    );
    *dirty_chunks = edit_buffer.merge_edits(&mut voxel_map.voxels);
}
