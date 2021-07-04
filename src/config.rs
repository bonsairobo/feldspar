use crate::{ChunkCacheConfig, MapConfig, RenderConfig};

use serde::Deserialize;

#[derive(Clone, Copy, Deserialize, Default)]
pub struct Config {
    pub map: MapConfig,
    pub render: RenderConfig,
    pub chunk_cache: ChunkCacheConfig,
    pub witness_radius: i32,
}

impl Config {
    pub fn read_file(path: &str) -> Result<Self, ron::Error> {
        let reader = std::fs::File::open(path)?;

        ron::de::from_reader(reader)
    }
}
