mod config;
mod loader;
mod witness;

pub use config::MapConfig;
pub use loader::LoaderConfig;
pub use witness::Witness;

use loader::loader_system;
use witness::witness_system;

use bevy::prelude::{CoreStage, IntoSystem, Plugin};

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
            .add_system_to_stage(CoreStage::Update, loader_system.system())
            .add_system_to_stage(CoreStage::Last, witness_system.system());
    }
}
