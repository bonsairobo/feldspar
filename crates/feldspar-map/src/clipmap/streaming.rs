mod load_search;
mod render_search;

use crate::clipmap::Level;

#[derive(Clone, Copy, Debug)]
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
    pub detail: f32,
    /// The [`Level`] where we detect new nodes and insert loading ancestor nodes.
    pub load_level: Level,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            detail: 6.0,
            load_level: 4,
        }
    }
}
