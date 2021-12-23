use crate::{
    clipmap::{ChunkClipMap, Level, NodePtr},
    coordinates::chunk_extent_at_level_vec3a,
    units::*,
};
use crate::core::geometry::Ray;
use crate::core::glam::IVec3;

use float_ord::FloatOrd;
use std::collections::BinaryHeap;

impl ChunkClipMap {
    pub fn earliest_ray_intersection(
        &self,
        ray: VoxelUnits<Ray>,
        min_level: Level,
    ) -> Option<(NodePtr, IVec3, [f32; 2])> {
        let mut heap = BinaryHeap::new();
        for (root_ptr, root_coords) in self.octree.iter_roots() {
            let extent = chunk_extent_at_level_vec3a(root_ptr.level(), ChunkUnits(root_coords));
            if let VoxelUnits(Some(time_window)) =
                VoxelUnits::map2(ray, extent, |r, e| r.cast_at_extent(e))
            {
                heap.push(RayTraceHeapElem {
                    ptr: root_ptr,
                    coords: root_coords,
                    time_window,
                });
            }
        }

        let mut earliest_entrance_time = f32::INFINITY;
        let mut earliest_elem: Option<RayTraceHeapElem> = None;

        while let Some(elem) = heap.pop() {
            if elem.ptr.level() == min_level && elem.time_window[0] < earliest_entrance_time {
                earliest_entrance_time = elem.time_window[0];
                earliest_elem = Some(elem);
                continue;
            }

            let mut is_leaf = true;
            self.octree.visit_children_with_coordinates(
                elem.ptr,
                elem.coords,
                |child_ptr, child_coords| {
                    is_leaf = false;
                    let extent =
                        chunk_extent_at_level_vec3a(child_ptr.level(), ChunkUnits(child_coords));
                    if let VoxelUnits(Some(time_window)) =
                        VoxelUnits::map2(ray, extent, |r, e| r.cast_at_extent(e))
                    {
                        if time_window[0] > earliest_entrance_time {
                            // Don't bother visiting children, they couldn't possibly have an earlier time if the parent
                            // doesn't.
                            return;
                        }
                        heap.push(RayTraceHeapElem {
                            ptr: child_ptr,
                            coords: child_coords,
                            time_window,
                        });
                    }
                },
            );

            // We're looking for the leaf node with the earliest intersection time.
            if is_leaf && elem.time_window[0] < earliest_entrance_time {
                earliest_entrance_time = elem.time_window[0];
                earliest_elem = Some(elem);
            }
        }

        earliest_elem.and_then(|elem| {
            (elem.time_window[1] >= elem.time_window[0])
                .then(|| (elem.ptr, elem.coords, elem.time_window))
        })
    }
}

#[derive(Clone, Copy)]
struct RayTraceHeapElem {
    ptr: NodePtr,
    coords: IVec3,
    time_window: [f32; 2],
}

impl RayTraceHeapElem {
    fn tmin(&self) -> f32 {
        self.time_window[0]
    }
}

impl PartialEq for RayTraceHeapElem {
    fn eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr
    }
}

impl Eq for RayTraceHeapElem {}

impl PartialOrd for RayTraceHeapElem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        FloatOrd(self.tmin())
            .partial_cmp(&FloatOrd(other.tmin()))
            .map(|o| o.reverse())
    }
}

impl Ord for RayTraceHeapElem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        FloatOrd(self.tmin()).cmp(&FloatOrd(other.tmin())).reverse()
    }
}
