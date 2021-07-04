//! This is just a fork of bevy_pbr that supports texture splatting and tri/biplanar mapping onto smooth voxel meshes.

mod assets;
mod debug_lines;
mod entity;
mod material;
mod mesh_generator;
mod render_graph;

pub use assets::*;
pub use entity::*;
pub use material::*;
pub use mesh_generator::*;

use debug_lines::debug_chunk_boundaries_system;
use render_graph::add_voxel_render_graph;

use crate::BevyState;

use bevy::app::prelude::*;
use bevy::asset::{AddAsset, Assets, Handle};
use bevy::render::{prelude::Color, shader};
use bevy::{ecs::system::IntoSystem, prelude::*};

use bevy_prototype_debug_lines::DebugLinesPlugin;

use serde::{Deserialize, Serialize};

#[derive(Default)]
pub struct VoxelRenderPlugin<S> {
    update_state: S,
    config: RenderConfig,
}

impl<S> VoxelRenderPlugin<S> {
    pub fn new(update_state: S, config: RenderConfig) -> Self {
        Self {
            update_state,
            config,
        }
    }
}

impl<S: BevyState> Plugin for VoxelRenderPlugin<S> {
    fn build(&self, app: &mut AppBuilder) {
        app.add_plugin(DebugLinesPlugin)
            .add_plugin(MeshGeneratorPlugin::new(self.update_state.clone()))
            .add_system_set(
                SystemSet::on_enter(self.update_state.clone())
                    .with_system(on_finished_loading.system()),
            );

        if self.config.debug_chunk_boundaries {
            app.add_system(debug_chunk_boundaries_system.system());
        }

        app.add_asset::<ArrayMaterial>().add_system_to_stage(
            CoreStage::PostUpdate,
            shader::asset_shader_defs_system::<ArrayMaterial>.system(),
        );
        add_voxel_render_graph(app.world_mut());

        // add default ArrayMaterial
        let mut materials = app
            .world_mut()
            .get_resource_mut::<Assets<ArrayMaterial>>()
            .unwrap();
        materials.set_untracked(
            Handle::<ArrayMaterial>::default(),
            ArrayMaterial {
                base_color: Color::PINK,
                unlit: true,
                ..Default::default()
            },
        );
    }
}

#[derive(Clone, Copy, Default, Deserialize, Serialize)]
pub struct RenderConfig {
    pub debug_chunk_boundaries: bool,
}
