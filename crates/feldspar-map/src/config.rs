use crate::clipmap::StreamingConfig;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize, Serialize)]
pub struct MapConfig {
    pub num_lods: u8,
    pub streaming: StreamingConfig,
}

impl Default for MapConfig {
    fn default() -> Self {
        Self {
            num_lods: 10,
            streaming: StreamingConfig::default(),
        }
    }
}
