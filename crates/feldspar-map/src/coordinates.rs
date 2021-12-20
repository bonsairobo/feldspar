use super::*;
use crate::{
    chunk::{CHUNK_SHAPE_IVEC3, CHUNK_SHAPE_LOG2_IVEC3, CHUNK_SHAPE_VEC3A},
    clipmap::{ChildIndex, Level},
    geometry::Sphere,
    units::*,
};

use grid_tree::{BranchShape, OctreeShapeI32};
use ilattice::glam::{IVec3, Vec3A};
use ilattice::prelude::Extent;

pub fn chunk_extent_from_min_ivec3(min: VoxelUnits<IVec3>) -> VoxelUnits<Extent<IVec3>> {
    min.map(|m| Extent::from_min_and_shape(m, CHUNK_SHAPE_IVEC3))
}

pub fn chunk_extent_from_min_vec3a(min: VoxelUnits<Vec3A>) -> VoxelUnits<Extent<Vec3A>> {
    min.map(|m| Extent::from_min_and_shape(m, CHUNK_SHAPE_VEC3A))
}

pub fn chunk_extent_at_level_vec3a(
    level: Level,
    coordinates: ChunkUnits<IVec3>,
) -> VoxelUnits<Extent<Vec3A>> {
    chunk_extent_at_level_ivec3(level, coordinates).map(|e| e.map_components(|c| c.as_vec3a()))
}

/// The extent in voxel coordinates of the chunk found at `(level, chunk coordinates)`.
pub fn chunk_extent_at_level_ivec3(
    level: Level,
    coordinates: ChunkUnits<IVec3>,
) -> VoxelUnits<Extent<IVec3>> {
    let min = coordinates.0 << level;
    let shape = CHUNK_SHAPE_IVEC3 << level;
    VoxelUnits(Extent::from_min_and_shape(min, shape))
}

pub fn chunk_min(coordinates: ChunkUnits<IVec3>) -> VoxelUnits<IVec3> {
    VoxelUnits(coordinates.0 << CHUNK_SHAPE_LOG2_IVEC3)
}

pub fn chunk_extent_ivec3(coordinates: ChunkUnits<IVec3>) -> VoxelUnits<Extent<IVec3>> {
    chunk_extent_from_min_ivec3(chunk_min(coordinates))
}

pub fn chunk_extent_vec3a(coordinates: ChunkUnits<IVec3>) -> VoxelUnits<Extent<Vec3A>> {
    chunk_extent_ivec3(coordinates).map(|e| e.map_components(|c| c.as_vec3a()))
}

/// Transforms a [`VoxelUnits`] extent `e` into a [`ChunkUnits`] extent `e'` that contains the coordinates of all chunks
/// intersected by `e`.
pub fn in_chunk_extent(e: VoxelUnits<Extent<IVec3>>) -> ChunkUnits<Extent<IVec3>> {
    ChunkUnits(Extent::from_min_and_max(
        e.0.minimum >> CHUNK_SHAPE_LOG2_IVEC3,
        e.0.max() >> CHUNK_SHAPE_LOG2_IVEC3,
    ))
}

/// Returns the [`ChunkUnits`] coordinates of the chunk that contains `p`.
pub fn in_chunk(p: VoxelUnits<IVec3>) -> ChunkUnits<IVec3> {
    ChunkUnits(p.0 >> CHUNK_SHAPE_LOG2_IVEC3)
}

pub fn ancestor_extent(levels_up: Level, extent: Extent<IVec3>) -> Extent<IVec3> {
    // We need the minimum to be an ancestor of (cover) the minimum.
    // We need the maximum to be an ancestor of (cover) the maximum.
    Extent::from_min_and_max(extent.minimum >> levels_up, extent.max() >> levels_up)
}

pub fn descendant_extent(levels_down: Level, extent: Extent<IVec3>) -> Extent<IVec3> {
    // Minimum and shape are simply multiplied.
    extent << levels_down
}

pub fn min_child_coords(parent_coords: IVec3) -> IVec3 {
    parent_coords << 1
}

pub fn parent_coords(child_coords: IVec3) -> IVec3 {
    child_coords >> 1
}

pub fn visit_children(parent_coords: IVec3, mut visitor: impl FnMut(ChildIndex, IVec3)) {
    let min_child = min_child_coords(parent_coords);
    for child_i in 0..8 {
        visitor(
            child_i,
            min_child + OctreeShapeI32::delinearize_child(child_i),
        );
    }
}

/// Returns a sphere at LOD0 that bounds the chunk at `(level, coords)`.
pub fn chunk_bounding_sphere(level: Level, coords: ChunkUnits<IVec3>) -> VoxelUnits<Sphere> {
    chunk_extent_at_level_ivec3(level, coords).map(|e| {
        let lod0_extent = descendant_extent(level, e);
        let center = (lod0_extent.minimum + (lod0_extent.shape >> 1i32)).as_vec3a();
        let radius = (lod0_extent.shape.max_element() >> 1) as f32 * 3f32.sqrt();
        Sphere { center, radius }
    })
}

/// Returns the extent covering all chunks at `level` which intersect `lod0_sphere`.
pub fn sphere_intersecting_ancestor_chunk_extent(
    lod0_sphere: VoxelUnits<Sphere>,
    level: Level,
) -> ChunkUnits<Extent<IVec3>> {
    let sphere_extent = in_chunk_extent(lod0_sphere.map(|s| s.aabb().containing_integer_extent()));
    sphere_extent.map(|e| ancestor_extent(level, e))
}
