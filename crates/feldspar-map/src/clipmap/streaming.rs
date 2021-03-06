mod load_search;
mod render_search;

pub use load_search::*;
pub use render_search::*;

use crate::units::VoxelUnits;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct StreamingConfig {
    /// A chunk is a *render candidate* if
    ///
    /// ```text
    /// D < R + clip_sphere.radius && (D / R) > detail
    /// ```
    ///
    /// where:
    ///
    ///   - `D` is the Euclidean distance from observer to the center of the chunk (in LOD0 space)
    ///   - `R` is the radius of the chunk's bounding sphere (in LOD0 space)
    pub detail: VoxelUnits<f32>,
    /// The radius of the clip [`Sphere`](crate::core::geometry::Sphere), i.e. the sphere centered at the observer outside of
    /// which terrain is not loaded.
    pub clip_sphere_radius: VoxelUnits<f32>,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            detail: VoxelUnits(6.0),
            clip_sphere_radius: VoxelUnits(1000.0),
        }
    }
}
