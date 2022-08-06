mod config;
mod loader;
mod witness;

use std::sync::Arc;
pub use config::MapConfig;
pub use loader::LoaderConfig;
pub use witness::Witness;

use loader::loader_system;
use witness::witness_system;

use bevy::prelude::{Commands, CoreStage, Plugin, Res};
use bevy::tasks::{IoTaskPool, TaskPoolBuilder};
use crate::clipmap::ChunkClipMap;
use crate::database::MapDb;
use crate::plugin::loader::PendingLoadTasks;

#[derive(Default)]
pub struct MapPlugin {
    config: MapConfig,
}

impl MapPlugin {
    pub fn new(config: MapConfig) -> Self {
        Self { config }
    }
}

impl Plugin for MapPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.insert_resource(self.config.clone())
            .add_startup_system(plugin_startup)
            .add_system_to_stage(CoreStage::Update, loader_system)
            .add_system_to_stage(CoreStage::Last, witness_system);
    }
}

fn plugin_startup(mut commands: Commands, config: Res<MapConfig>) {
    let db = sled::Config::default()
        .path("tmp".to_owned())
        .use_compression(false)
        .mode(sled::Mode::LowSpace)
        .open()
        .expect("Failed to open world DB");

    let mapdb = MapDb::open(&db, "main").expect("Failed to load main level");
    commands.insert_resource(
        Arc::new(mapdb)
    );
    let chunk_clip_map = ChunkClipMap::new(config.num_lods, config.streaming);
    commands.insert_resource(chunk_clip_map);

    let task_pool = PendingLoadTasks::new();
    commands.insert_resource(task_pool);
}