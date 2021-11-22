# feldspar

The Feldspar voxel plugin for Bevy Engine.

This crate provides these plugins:

- [`VoxelWorldPlugin`](crate::world::VoxelWorldPlugin): The top-level plugin which manages plugin state and relies on
  lower-level plugins.
  - [`VoxelDataPlugin`](crate::voxel_data::VoxelDataPlugin): Manages access to the
    [`SdfVoxelMap`](crate::voxel_data::SdfVoxelMap), which involves compression, caching, and persistence.
  - [`VoxelRenderPlugin`](crate::renderer::VoxelRenderPlugin): Generates and renders meshes from voxel chunks using
    isosurface extraction and biplanar texture mapping.
  - [`BvtPlugin`](crate::bvt::BvtPlugin): Maintains voxel bounding volume hierarchies for spatial queries.
