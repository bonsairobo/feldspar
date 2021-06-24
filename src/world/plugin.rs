use super::chunk_loader_system;

use crate::{BevyState, BvtPlugin, Config, VoxelDataPlugin, VoxelRenderPlugin};

use bevy::{app::prelude::*, ecs::prelude::*};

pub struct VoxelWorldPlugin<S> {
    config: Config,
    update_state: S,
}

impl<S> VoxelWorldPlugin<S> {
    pub fn new(config: Config, update_state: S) -> Self {
        Self {
            config,
            update_state,
        }
    }
}

impl<S: BevyState> Plugin for VoxelWorldPlugin<S> {
    fn build(&self, app: &mut AppBuilder) {
        app.insert_resource(self.config)
            .add_plugin(VoxelDataPlugin::new(
                self.config.map,
                self.config.chunk_cache,
            ))
            .add_plugin(VoxelRenderPlugin::new(self.update_state.clone()))
            .add_plugin(BvtPlugin)
            .add_system_set(
                SystemSet::on_update(self.update_state.clone())
                    .with_system(chunk_loader_system.system()),
            );
    }
}
