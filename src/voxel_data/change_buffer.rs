use crate::{
    empty_sdf_chunk_hash_map, voxel_data::database::LoadedSuperChunk,
    CompressibleSdfChunkMapReader, SdfArray, SdfChunkHashMap, SdfVoxelMap, VoxelType,
    EMPTY_SDF_VOXEL,
};

use bevy::prelude::*;
use building_blocks::{prelude::*, storage::SmallKeyHashSet};

/// An ephemeral buffer that stores map changes for a single frame.
///
/// For the sake of pipelining, all voxels loaded or edited are first written out of place here. They will later be merged into
/// the world map.
pub struct ChangeBuffer {
    edited_chunks: SdfChunkHashMap,
    edited_extents: Vec<Extent3i>,
    /// The superchunks that have been loaded from the database this frame.
    loaded_superchunks: Vec<LoadedSuperChunk>,
    /// The superchunks that have been unloaded from the world map this frame.
    unloaded_superchunks: Vec<Octant>,
}

impl ChangeBuffer {
    pub fn new(chunk_shape: Point3i) -> Self {
        Self {
            edited_chunks: empty_sdf_chunk_hash_map(chunk_shape),
            edited_extents: Vec::new(),
            loaded_superchunks: Vec::new(),
            unloaded_superchunks: Vec::new(),
        }
    }

    pub fn has_data(&self) -> bool {
        !(self.edited_chunks.storage().is_empty() && self.loaded_superchunks.is_empty())
    }

    pub fn unload_superchunk(&mut self, octant: Octant) {
        self.unloaded_superchunks.push(octant);
    }

    pub fn load_superchunk(&mut self, superchunk: LoadedSuperChunk) {
        self.loaded_superchunks.push(superchunk);
    }

    /// This function does read-modify-write of the voxels in `extent`. If a chunk is missing from the backbuffer, it will be
    /// copied from the `reader` before being written.
    ///
    /// All chunks in the Moore Neighborhood of any edited chunk will be marked as dirty. This is necessary because there are
    /// dependencies between adjacent chunks that must be considered during post-processing (e.g. during mesh generation).
    pub fn edit_voxels_out_of_place(
        &mut self,
        reader: &CompressibleSdfChunkMapReader,
        extent: Extent3i,
        edit_func: impl FnMut(Point3i, (&mut VoxelType, &mut Sd8)),
    ) {
        debug_assert!(reader.chunk_shape().eq(&self.edited_chunks.chunk_shape()));

        // Copy any of the overlapping chunks that don't already exist in the backbuffer, i.e. those chunks which haven't been
        // modified yet.
        for chunk_min in reader.indexer.chunk_mins_for_extent(&extent) {
            let chunk_key = ChunkKey::new(0, chunk_min);
            self.edited_chunks
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

        self.edited_extents.push(extent);

        // Edit the backbuffer.
        self.edited_chunks
            .lod_view_mut(0)
            .for_each_mut(&extent, edit_func);
    }

    pub fn write_chunk(&mut self, chunk_min: Point3i, chunk: SdfArray) {
        self.edited_chunks
            .write_chunk(ChunkKey::new(0, chunk_min), chunk);
        self.edited_extents.push(
            self.edited_chunks
                .indexer
                .extent_for_chunk_with_min(chunk_min),
        );
    }

    /// Write all of the new chunks into `dst_map`. Returns the dirty chunks.
    fn merge_changes(self, dst_map: &mut SdfVoxelMap) -> DirtyChunks {
        let ChangeBuffer {
            edited_chunks,
            mut edited_extents,
            loaded_superchunks,
            unloaded_superchunks,
        } = self;

        let SdfVoxelMap {
            voxels,
            chunk_index,
            ..
        } = dst_map;

        let indexer = edited_chunks.indexer;
        let chunk_storage = edited_chunks.take_storage();
        let mut changed_chunk_mins: Vec<_> =
            chunk_storage.chunk_keys().map(|k| k.minimum).collect();

        for (chunk_key, chunk) in chunk_storage.into_iter() {
            voxels.write_chunk(chunk_key, chunk);
        }

        // TODO: we currently assume that edits are disjoint from loaded superchunks; is this legit?
        for LoadedSuperChunk { octant, chunks } in loaded_superchunks.into_iter() {
            let key_iter = chunks.into_iter().map(|(chunk_key, chunk)| {
                voxels.write_chunk(chunk_key, chunk);
                changed_chunk_mins.push(chunk_key.minimum);
                edited_extents.push(indexer.extent_for_chunk_with_min(chunk_key.minimum));

                chunk_key
            });
            chunk_index.insert_superchunk(octant.minimum(), key_iter);
        }

        for octant in unloaded_superchunks.into_iter() {
            dst_map.unload_superchunk(octant, |chunk_key| {
                changed_chunk_mins.push(chunk_key.minimum);
                edited_extents.push(indexer.extent_for_chunk_with_min(chunk_key.minimum));
            });
        }

        let mut dirty_chunk_mins = SmallKeyHashSet::new();
        for extent in edited_extents.into_iter() {
            // Mark the chunks and their neighbors as dirty.
            let chunk_shape = indexer.chunk_shape();
            let dirty_extent = Extent3i::from_min_and_max(
                extent.minimum - chunk_shape,
                extent.max() + chunk_shape,
            );
            for chunk_key in indexer.chunk_mins_for_extent(&dirty_extent) {
                dirty_chunk_mins.insert(chunk_key);
            }
        }

        DirtyChunks {
            changed_chunk_mins,
            dirty_chunk_mins,
        }
    }
}

/// The sets of chunk keys that have either been loaded, edited directly, or marked as dirty by virtue of neighboring a changed
/// chunk.
#[derive(Default)]
pub struct DirtyChunks {
    changed_chunk_mins: Vec<Point3i>,
    /// Includes the changed chunks as well as their neighbors, all of which need to be re-meshed.
    dirty_chunk_mins: SmallKeyHashSet<Point3i>,
}

impl DirtyChunks {
    pub fn changed_chunk_mins(&self) -> &[Point3i] {
        &self.changed_chunk_mins
    }

    pub fn dirty_chunk_mins(&self) -> &SmallKeyHashSet<Point3i> {
        &self.dirty_chunk_mins
    }
}

/// Merges changes from the `ChangeBuffer` into the `SdfVoxelMap`. By setting the `DirtyChunks` resource, the chunk
/// post-processing systems will be notified to process dirty chunks on the next frame.
pub fn double_buffering_system(
    mut voxel_map: ResMut<SdfVoxelMap>,
    mut change_buffer: ResMut<ChangeBuffer>,
    mut dirty_chunks: ResMut<DirtyChunks>,
) {
    let change_buffer = std::mem::replace(
        &mut *change_buffer,
        ChangeBuffer::new(voxel_map.voxels.chunk_shape()),
    );
    *dirty_chunks = change_buffer.merge_changes(&mut voxel_map);
}
