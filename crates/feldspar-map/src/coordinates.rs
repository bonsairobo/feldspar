use super::*;
use crate::{ChildIndex, Sphere, CHUNK_SHAPE_IVEC3, CHUNK_SHAPE_VEC3A};

use grid_tree::{BranchShape, OctreeShapeI32};
use ilattice::glam::{IVec3, Vec3A};
use ilattice::prelude::Extent;

pub fn chunk_extent_ivec3_from_min(min: IVec3) -> Extent<IVec3> {
    Extent::from_min_and_shape(min, CHUNK_SHAPE_IVEC3)
}

pub fn chunk_extent_vec3a_from_min(min: Vec3A) -> Extent<Vec3A> {
    Extent::from_min_and_shape(min, CHUNK_SHAPE_VEC3A)
}

pub fn chunk_extent_vec3a(level: Level, coordinates: IVec3) -> Extent<Vec3A> {
    chunk_extent_ivec3(level, coordinates).map_components(|c| c.as_vec3a())
}

/// The extent in voxel coordinates of the chunk found at `(level, chunk coordinates)`.
pub fn chunk_extent_ivec3(level: Level, coordinates: IVec3) -> Extent<IVec3> {
    let min = coordinates << level;
    let shape = CHUNK_SHAPE_IVEC3 << level;
    Extent::from_min_and_shape(min, shape)
}

/// Transforms a world-space extent `e` into a chunk-space extent `e'` that contains the coordinates of all chunks intersected
/// by `e`.
pub fn in_chunk_extent(e: Extent<IVec3>) -> Extent<IVec3> {
    Extent::from_min_and_max(
        e.minimum >> CHUNK_SHAPE_LOG2_IVEC3,
        e.max() >> CHUNK_SHAPE_LOG2_IVEC3,
    )
}

/// Returns the "chunk coordinates" of the chunk that contains `p`.
pub fn in_chunk(p: IVec3) -> IVec3 {
    p >> CHUNK_SHAPE_LOG2_IVEC3
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
pub fn chunk_bounding_sphere(level: Level, coords: IVec3) -> Sphere {
    let level_extent = chunk_extent_ivec3(level, coords);
    let lod0_extent = descendant_extent(level, level_extent);
    let center = (lod0_extent.minimum + (lod0_extent.shape >> 1i32)).as_vec3a();

    let radius = (lod0_extent.shape.max_element() >> 1) as f32 * 3f32.sqrt();

    Sphere { center, radius }
}
