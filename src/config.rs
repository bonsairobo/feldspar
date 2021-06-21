use crate::ChunkCacheConfig;

use building_blocks::core::Point3i;
use serde::Deserialize;

#[derive(Clone, Copy, Deserialize, Default)]
pub struct Config {
    pub chunk_shape: Point3i,
    pub chunk_cache: ChunkCacheConfig,
    pub wireframes: bool,
}

impl Config {
    pub fn read_file(path: &str) -> Result<Self, ron::Error> {
        let reader = std::fs::File::open(path)?;

        ron::de::from_reader(reader)
    }
}
