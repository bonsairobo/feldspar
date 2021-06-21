mod entity;
mod material;
mod mesh_generator;
mod render_graph;

pub use entity::*;
pub use material::*;
pub use mesh_generator::*;

use render_graph::add_voxel_render_graph;

use crate::BevyState;

use bevy::app::prelude::*;
use bevy::asset::{AddAsset, Assets, Handle};
use bevy::ecs::system::IntoSystem;
use bevy::render::{prelude::Color, shader};

#[derive(Default)]
pub struct VoxelRenderPlugin<S> {
    loaded_state: S,
}

impl<S> VoxelRenderPlugin<S> {
    pub fn new(loaded_state: S) -> Self {
        Self { loaded_state }
    }
}

impl<S: BevyState> Plugin for VoxelRenderPlugin<S> {
    fn build(&self, app: &mut AppBuilder) {
        app.add_plugin(MeshGeneratorPlugin::new(self.loaded_state.clone()));

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
