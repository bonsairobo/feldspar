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

    pub fn set_velocity(&mut self, v: Vec3A) {
        self.velocity = v;
        self.inverse_velocity = 1.0 / self.velocity;
    }

    /// If the ray intersects box `b`, returns `(tmin, tmax)`, the entrance and exit times of the ray.
    ///
    /// Implemented as branchless "slab method". Does not attempt to handle NaNs properly.
    ///
    /// Refer to: https://tavianator.com/2015/ray_box_nan.html
    pub fn cast_at_aabb(&self, b: Extent<Vec3A>) -> Option<[f32; 2]> {
        let blub = b.least_upper_bound();

        let mut t1 = (b.minimum[0] - self.start[0]) * self.inverse_velocity[0];
        let mut t2 = (blub[0] - self.start[0]) * self.inverse_velocity[0];

        let mut tmin = t1.min(t2);
        let mut tmax = t1.max(t2);

        for i in 1..3 {
            t1 = (b.minimum[i] - self.start[i]) * self.inverse_velocity[i];
            t2 = (blub[i] - self.start[i]) * self.inverse_velocity[i];

            tmin = t1.min(t2).max(tmin);
            tmax = t1.max(t2).min(tmax);
        }

        (tmax > tmin.max(0.0)).then(|| [tmin, tmax])
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
