use crate::prelude::{ArrayMaterial, MeshMaterial};

use bevy::{
    asset::prelude::*,
    ecs::prelude::*,
    render::{prelude::*, texture::AddressMode},
};

/// The set of assets needed by the renderer. This resource must exist and all asset handles must have finished loading before
/// entering the update state configured by `VoxelWorldPlugin::new`.
pub struct VoxelRenderAssets {
    /// A 2D texture containing vertically stacked images of the same size. Each image corresponds to one voxel type.
    pub mesh_base_color: Handle<Texture>,
}

pub fn on_finished_loading(
    assets: Res<VoxelRenderAssets>,
    mut commands: Commands,
    mut array_materials: ResMut<Assets<ArrayMaterial>>,
    mut textures: ResMut<Assets<Texture>>,
) {
    let texture = textures
        .get_mut(&assets.mesh_base_color)
        .expect("mesh_base_color texture does not exist");

    handle_loaded_array_texture(texture, 4);

    let mut material = ArrayMaterial::from(assets.mesh_base_color.clone());
    material.roughness = 0.8;
    material.reflectance = 0.2;
    commands.insert_resource(MeshMaterial(array_materials.add(material)));
}

fn handle_loaded_array_texture(texture: &mut Texture, num_layers: u32) {
    texture.reinterpret_stacked_2d_as_array(num_layers);
    texture.sampler.address_mode_u = AddressMode::Repeat;
    texture.sampler.address_mode_v = AddressMode::Repeat;
}
