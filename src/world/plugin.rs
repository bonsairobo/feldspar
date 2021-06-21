use crate::{BevyState, BvtPlugin, Config, VoxelDataPlugin, VoxelRenderPlugin};

use bevy::app::prelude::*;

pub struct VoxelWorldPlugin<S> {
    config: Config,
    loaded_state: S,
}

impl<S> VoxelWorldPlugin<S> {
    pub fn new(config: Config, loaded_state: S) -> Self {
        Self {
            config,
            loaded_state,
        }
    }
}

impl<S: BevyState> Plugin for VoxelWorldPlugin<S> {
    fn build(&self, app: &mut AppBuilder) {
        app.add_plugin(VoxelDataPlugin::new(
            self.config.chunk_shape,
            self.config.chunk_cache,
        ))
        .add_plugin(VoxelRenderPlugin::new(self.loaded_state.clone()))
        .add_plugin(BvtPlugin);
    }
}
