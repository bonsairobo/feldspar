use glam::Vec3A;
use ilattice::extent::Extent;

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

    /// If the ray intersects box `aabb`, returns `(tmin, tmax)`, the entrance and exit times of the ray.
    ///
    /// Implemented as branchless, vectorized "slab method". Does not attempt to handle NaNs properly.
    ///
    /// Refer to: https://tavianator.com/2015/ray_box_nan.html
    pub fn cast_at_aabb(&self, aabb: Extent<Vec3A>) -> Option<[f32; 2]> {
        let blub = aabb.least_upper_bound();

        let t1 = (aabb.minimum - self.start) * self.inverse_velocity;
        let t2 = (blub - self.start) * self.inverse_velocity;

        let tmin = t1.min(t2).max_element();
        let tmax = t1.max(t2).min_element();

        (tmax >= tmin.max(0.0)).then(|| [tmin, tmax])
    }
}

pub struct Sphere {
    pub center: Vec3A,
    pub radius: f32,
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

        assert_eq!(ray.cast_at_aabb(aabb), None);
    }

    #[test]
    fn cast_ray_at_aabb_hits() {
        let ray = Ray::new(Vec3A::ONE, Vec3A::new(1.0, 1.0, 1.0));

        let aabb = Extent::from_min_and_lub(Vec3A::splat(1.1), Vec3A::splat(2.0));

        let [tmin, tmax] = ray.cast_at_aabb(aabb).unwrap();
        assert_relative_eq!(tmin, 0.1);
        assert_relative_eq!(tmax, 1.0);
    }
}
