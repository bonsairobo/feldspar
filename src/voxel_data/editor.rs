use super::edit_buffer::EditBuffer;
use crate::{SdfArray, SdfVoxelMap, ThreadLocalVoxelCache, VoxelType};
use bevy::ecs::{prelude::*, system::SystemParam};
use building_blocks::prelude::*;

/// A `SystemParam` that double-buffers writes to the `SdfVoxelMap` and detects which chunks are changed each frame. On the
/// subsequent frame, the set of dirty and edited chunk keys will be available in the `DirtyChunks` resource.
#[derive(SystemParam)]
pub struct VoxelEditor<'a> {
    pub map: Res<'a, SdfVoxelMap>,
    pub local_cache: Res<'a, ThreadLocalVoxelCache>,
    edit_buffer: ResMut<'a, EditBuffer>,
}

impl<'a> VoxelEditor<'a> {
    /// Run `edit_func` on all voxels in `extent`. Does not mark the neighbors of edited chunks.
    pub fn edit_extent(
        &mut self,
        extent: Extent3i,
        edit_func: impl FnMut(Point3i, (&mut VoxelType, &mut Sd8)),
    ) {
        self._edit_extent(false, extent, edit_func);
    }

    /// Run `edit_func` on all voxels in `extent`. All edited chunks and their neighbors will be
    /// marked as dirty.
    pub fn edit_extent_and_touch_neighbors(
        &mut self,
        extent: Extent3i,
        edit_func: impl FnMut(Point3i, (&mut VoxelType, &mut Sd8)),
    ) {
        self._edit_extent(true, extent, edit_func);
    }

    fn _edit_extent(
        &mut self,
        touch_neighbors: bool,
        extent: Extent3i,
        edit_func: impl FnMut(Point3i, (&mut VoxelType, &mut Sd8)),
    ) {
        let tls = self.local_cache.get();
        let reader = self.map.reader(&tls);
        self.edit_buffer
            .edit_voxels_out_of_place(&reader, extent, edit_func, touch_neighbors);
    }

    pub fn insert_chunk_and_touch_neighbors(&mut self, chunk_key: Point3i, chunk: SdfArray) {
        self.edit_buffer.insert_chunk(true, chunk_key, chunk);
    }

    pub fn insert_chunk(&mut self, chunk_key: Point3i, chunk: SdfArray) {
        self.edit_buffer.insert_chunk(false, chunk_key, chunk);
    }
}
