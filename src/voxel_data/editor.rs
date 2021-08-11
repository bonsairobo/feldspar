use super::map_changes::FrameMapChanges;
use crate::prelude::{SdfArray, SdfVoxelMap, VoxelType};
use bevy::ecs::{prelude::*, system::SystemParam};
use building_blocks::prelude::*;

/// A `SystemParam` that double-buffers writes to the `SdfVoxelMap` and detects which chunks are changed each frame. On the
/// subsequent frame, the set of dirty and edited chunk keys will be available in the `DirtyChunks` resource.
#[derive(SystemParam)]
pub struct VoxelEditor<'a> {
    pub map: Res<'a, SdfVoxelMap>,
    frame_changes: ResMut<'a, FrameMapChanges>,
}

impl<'a> VoxelEditor<'a> {
    /// Run `edit_func` on all voxels in `extent`. All edited chunks and their neighbors will be marked as dirty.
    pub fn edit_extent_and_touch_neighbors(
        &mut self,
        extent: Extent3i,
        edit_func: impl FnMut(Point3i, (&mut VoxelType, &mut Sd8)),
    ) {
        self._edit_extent(extent, edit_func);
    }

    fn _edit_extent(
        &mut self,
        extent: Extent3i,
        edit_func: impl FnMut(Point3i, (&mut VoxelType, &mut Sd8)),
    ) {
        self.frame_changes
            .edit_voxels_out_of_place(&self.map.voxels, extent, edit_func);
    }

    pub fn write_chunk_and_touch_neighbors(&mut self, chunk_min: Point3i, chunk: SdfArray) {
        self.frame_changes.write_chunk(chunk_min, chunk);
    }

    pub fn frame_changes_has_data(&self) -> bool {
        self.frame_changes.has_data()
    }
}
