use crate::glam::Vec3A;
use crate::ilattice::prelude::Extent;

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
}

#[derive(Clone, Copy, Debug)]
pub struct Sphere {
    pub center: Vec3A,
    pub radius: f32,
}

impl Sphere {
    pub fn new(center: Vec3A, radius: f32) -> Self {
        Self { center, radius }
    }

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
}
