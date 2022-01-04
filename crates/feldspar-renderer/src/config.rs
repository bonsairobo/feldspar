use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize, Serialize)]
pub struct RenderConfig {
    /// A number in [0, 100] determining the percentage of a total frame's CPU time allocated for chunk meshing.
    pub mesh_generation_frame_time_budget_pct: u8,
    pub wireframes: bool,
    pub lod_colors: bool,
    pub msaa: Option<u32>,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            mesh_generation_frame_time_budget_pct: 20,
            wireframes: false,
            lod_colors: false,
            msaa: Some(4), // # samples
        }
    }
}
