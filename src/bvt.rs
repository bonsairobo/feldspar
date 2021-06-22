use crate::{DirtyChunks, EmptyChunks, SdfVoxelMap, ThreadLocalVoxelCache};

use building_blocks::{prelude::*, search::OctreeDbvt, storage::OctreeSet};

use bevy::{
    prelude::*,
    tasks::{ComputeTaskPool, TaskPool},
};

/// Manages the `VoxelBvt` resource by generating `OctreeSet`s for any edited chunks. Depends on the `VoxelDataPlugin`.
#[derive(Default)]
pub struct BvtPlugin;

impl Plugin for BvtPlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.insert_resource(VoxelBvt::default())
            .add_system(octree_generator_system.system());
    }
}

/// An `OctreeDbvt` that maps chunk keys to chunk `OctreeSet`s.
pub type VoxelBvt = OctreeDbvt<Point3i>;

/// Generates new octrees for all edited chunks.
fn octree_generator_system(
    pool: Res<ComputeTaskPool>,
    voxel_map: Res<SdfVoxelMap>,
    local_caches: Res<ThreadLocalVoxelCache>,
    dirty_chunks: Res<DirtyChunks>,
    mut voxel_bvt: ResMut<VoxelBvt>,
    mut empty_chunks: ResMut<EmptyChunks>,
) {
    let new_chunk_octrees =
        generate_octree_for_each_chunk(&*dirty_chunks, &*voxel_map, &*local_caches, &*pool);

    let new_empty_chunks: Vec<Point3i> = new_chunk_octrees
        .iter()
        .filter_map(|(chunk_min, octree)| {
            if octree.is_empty() {
                Some(*chunk_min)
            } else {
                None
            }
        })
        .collect();

    for (chunk_min, octree) in new_chunk_octrees.into_iter() {
        if octree.is_empty() {
            voxel_bvt.remove(&chunk_min);
        } else {
            log::debug!("Inserting chunk OctreeBvt for {:?}", chunk_min);
            voxel_bvt.insert(chunk_min, octree);
        }
    }

    // We want to delete any SDF chunks that are not adjacent to a non-empty chunk.
    // Otherwise, if an empty chunk is adjacent to a non-empty chunk, then it may actually have influence over the shape of the
    // adjacent mesh. (Positive values are "empty," but used for surface interpolation).
    let neighborhood = Point3i::MOORE_OFFSETS;
    let chunk_shape = voxel_map.voxels.chunk_shape();
    for chunk_min in new_empty_chunks.into_iter() {
        // See if there are any adjacent non-empty chunks.
        let mut all_neighbors_empty = true;
        for offset in neighborhood.iter().cloned() {
            let neighbor = chunk_min + offset * chunk_shape;
            if voxel_bvt.contains_key(&neighbor) {
                // We found a non-empty neighbor.
                all_neighbors_empty = false;
                break;
            }
        }

        if all_neighbors_empty {
            empty_chunks.mark_for_removal(ChunkKey::new(0, chunk_min));
        }
    }
}

fn generate_octree_for_each_chunk(
    dirty_chunks: &DirtyChunks,
    map: &SdfVoxelMap,
    local_caches: &ThreadLocalVoxelCache,
    pool: &TaskPool,
) -> Vec<(Point3i, OctreeSet)> {
    pool.scope(|s| {
        for chunk_min in dirty_chunks.edited_chunk_mins.clone().into_iter() {
            s.spawn(async move {
                let cache_tls = local_caches.get();
                let reader = map.reader(&cache_tls);
                let chunk = reader.get_chunk(ChunkKey::new(0, chunk_min)).unwrap();
                let transform_chunk = TransformMap::new(chunk, map.voxel_info_transform());

                (
                    chunk_min,
                    OctreeSet::from_array3(&transform_chunk, *chunk.extent()),
                )
            })
        }
    })
}
