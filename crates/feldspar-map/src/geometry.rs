use crate::{chunk_min, chunk_extent_vec3a, Chunk, ChunkShape, PaletteId8, Sd8, CHUNK_SIZE, ChunkUnits};

use grid_ray::GridRayIter3;
use ilattice::glam::{IVec3, Vec3A};
use ilattice::prelude::Extent;
use ndshape::ConstShape;

#[derive(Clone, Copy)]
pub struct Ray {
    pub start: Vec3A,
    velocity: Vec3A,
    inverse_velocity: Vec3A,
}

impl Ray {
    pub fn new(start: Vec3A, velocity: Vec3A) -> Self {
        Self {
            start,
            velocity,
            inverse_velocity: 1.0 / velocity,
        }
    }

    pub fn velocity(&self) -> Vec3A {
        self.velocity
    }

    pub fn inverse_velocity(&self) -> Vec3A {
        self.inverse_velocity
    }

    pub fn position_at(&self, t: f32) -> Vec3A {
        self.start + t * self.velocity
    }

    /// If the ray intersects box `aabb`, returns `(tmin, tmax)`, the entrance and exit times of the ray.
    ///
    /// Implemented as branchless, vectorized "slab method". Does not attempt to handle NaNs properly.
    ///
    /// Refer to [this reference](https://tavianator.com/2015/ray_box_nan.html).
    pub fn cast_at_extent(&self, aabb: Extent<Vec3A>) -> Option<[f32; 2]> {
        let blub = aabb.least_upper_bound();

        let t1 = (aabb.minimum - self.start) * self.inverse_velocity;
        let t2 = (blub - self.start) * self.inverse_velocity;

        let tmin = t1.min(t2).max_element();
        let tmax = t1.max(t2).min_element();

        (tmax >= tmin.max(0.0)).then(|| [tmin, tmax])
    }

    /// Visit every voxel in `chunk` that intersects the ray. Return `false` to stop the traversal.
    pub fn cast_through_chunk(
        &self,
        chunk_coords: ChunkUnits<IVec3>,
        chunk: &Chunk,
        mut visitor: impl FnMut(f32, IVec3, Sd8, PaletteId8) -> bool,
    ) {
        let chunk_aabb = chunk_extent_vec3a(chunk_coords).into_inner();
        if let Some([t_enter_chunk, t_exit_chunk]) = self.cast_at_extent(chunk_aabb) {
            // Nudge the start and end a little bit to be sure we stay in the chunk.
            let duration_inside_chunk = t_exit_chunk - t_enter_chunk;
            let nudge_duration = 0.000001 * duration_inside_chunk;
            let t_nudge_start = t_enter_chunk + nudge_duration;
            let nudge_start = self.position_at(t_nudge_start);

            if !chunk_aabb.contains(nudge_start) {
                return;
            }

            let chunk_min = chunk_min(chunk_coords).into_inner();
            let nudge_t_max = t_exit_chunk - nudge_duration;
            let iter = GridRayIter3::new(nudge_start, self.velocity);
            for (t_enter, p) in iter {
                // We technically "advanced the clock" by t_nudge_start before we started this iterator.
                let actual_t_enter = t_enter + t_nudge_start;
                if actual_t_enter > nudge_t_max {
                    break;
                }
                let offset = p - chunk_min;
                let index = ChunkShape::linearize(offset.to_array()) as usize;
                if index >= CHUNK_SIZE {
                    // Floating Point Paranoia: Just avoid panicking from out-of-bounds access at all costs.
                    // TODO: log warning!
                    break;
                }
                if !visitor(
                    actual_t_enter,
                    p,
                    chunk.sdf[index],
                    chunk.palette_ids[index],
                ) {
                    break;
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Sphere {
    pub center: Vec3A,
    pub radius: f32,
}

impl Sphere {
    pub fn contains(&self, other: &Self) -> bool {
        let dist = self.center.distance(other.center);
        dist + other.radius < self.radius
    }

    pub fn intersects(&self, other: &Self) -> bool {
        let dist = self.center.distance(other.center);
        dist - other.radius < self.radius
    }

    pub fn aabb(&self) -> Extent<Vec3A> {
        Extent::from_min_and_shape(Vec3A::splat(-self.radius), Vec3A::splat(2.0 * self.radius))
            + self.center
    }
}

// ████████╗███████╗███████╗████████╗
// ╚══██╔══╝██╔════╝██╔════╝╚══██╔══╝
//    ██║   █████╗  ███████╗   ██║
//    ██║   ██╔══╝  ╚════██║   ██║
//    ██║   ███████╗███████║   ██║
//    ╚═╝   ╚══════╝╚══════╝   ╚═╝

#[cfg(test)]
mod test {
    use crate::AMBIENT_SD8;

    use super::*;

    use approx::assert_relative_eq;

    #[test]
    fn cast_ray_at_aabb_misses() {
        let ray = Ray::new(Vec3A::ONE, Vec3A::new(1.0, 0.0, 0.0));

        let aabb = Extent::from_min_and_lub(Vec3A::splat(1.1), Vec3A::splat(2.0));

        assert_eq!(ray.cast_at_extent(aabb), None);
    }

    #[test]
    fn cast_ray_at_aabb_hits() {
        let ray = Ray::new(Vec3A::ONE, Vec3A::new(1.0, 1.0, 1.0));

        let aabb = Extent::from_min_and_lub(Vec3A::splat(1.1), Vec3A::splat(2.0));

        let [tmin, tmax] = ray.cast_at_extent(aabb).unwrap();
        assert_relative_eq!(tmin, 0.1);
        assert_relative_eq!(tmax, 1.0);
    }

    #[test]
    fn cast_into_chunk_and_hit_voxel() {
        let ray = Ray::new(Vec3A::ONE, Vec3A::new(1.0, 1.0, 1.0));
        let mut chunk = Chunk::default();
        let chunk_coords = ChunkUnits(IVec3::ZERO);
        let chunk_min = chunk_min(chunk_coords).into_inner();

        // Mark one voxel in the middle to prove that we can hit it and stop.
        chunk.palette_view_mut()[IVec3::new(7, 7, 7) - chunk_min] = 1;

        let mut visited_coords = Vec::new();
        ray.cast_through_chunk(chunk_coords, &chunk, |_t_enter, coords, sdf, palette_id| {
            assert_eq!(sdf, AMBIENT_SD8);
            visited_coords.push(coords);
            palette_id == 0
        });

        assert_eq!(
            visited_coords.as_slice(),
            &[
                IVec3::new(0, 0, 0),
                IVec3::new(0, 0, 1),
                IVec3::new(0, 1, 1),
                IVec3::new(1, 1, 1),
                IVec3::new(1, 1, 2),
                IVec3::new(1, 2, 2),
                IVec3::new(2, 2, 2),
                IVec3::new(2, 2, 3),
                IVec3::new(2, 3, 3),
                IVec3::new(3, 3, 3),
                IVec3::new(3, 3, 4),
                IVec3::new(3, 4, 4),
                IVec3::new(4, 4, 4),
                IVec3::new(4, 4, 5),
                IVec3::new(4, 5, 5),
                IVec3::new(5, 5, 5),
                IVec3::new(5, 5, 6),
                IVec3::new(5, 6, 6),
                IVec3::new(6, 6, 6),
                IVec3::new(6, 6, 7),
                IVec3::new(6, 7, 7),
                IVec3::new(7, 7, 7),
            ]
        );
    }

    #[test]
    fn cast_through_chunk() {
        let ray = Ray::new(Vec3A::new(-0.5, 0.5, 0.5), Vec3A::new(1.0, 0.0, 0.0));
        let chunk = Chunk::default();
        let chunk_coords = ChunkUnits(IVec3::ZERO);

        let [t_chunk_enter, t_chunk_exit] = ray
            .cast_at_extent(chunk_extent_vec3a(chunk_coords).into_inner())
            .unwrap();

        let mut visited_coords = Vec::new();
        ray.cast_through_chunk(chunk_coords, &chunk, |t_enter, coords, sdf, palette_id| {
            assert_eq!(sdf, AMBIENT_SD8);
            assert_eq!(palette_id, 0);
            assert!(t_enter >= t_chunk_enter);
            assert!(t_enter < t_chunk_exit);
            visited_coords.push(coords);
            true
        });

        assert_eq!(
            visited_coords.as_slice(),
            &[
                IVec3::new(0, 0, 0),
                IVec3::new(1, 0, 0),
                IVec3::new(2, 0, 0),
                IVec3::new(3, 0, 0),
                IVec3::new(4, 0, 0),
                IVec3::new(5, 0, 0),
                IVec3::new(6, 0, 0),
                IVec3::new(7, 0, 0),
                IVec3::new(8, 0, 0),
                IVec3::new(9, 0, 0),
                IVec3::new(10, 0, 0),
                IVec3::new(11, 0, 0),
                IVec3::new(12, 0, 0),
                IVec3::new(13, 0, 0),
                IVec3::new(14, 0, 0),
                IVec3::new(15, 0, 0),
            ]
        );
    }
}
