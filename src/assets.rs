use crate::{ArrayMaterial, MeshMaterial};

use bevy::{
    asset::prelude::*,
    ecs::prelude::*,
    render::{prelude::*, texture::AddressMode},
};

struct LoadingTexture(Handle<Texture>);

fn start_loading(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(LoadingTexture(
        asset_server.load("grass_rock_snow_dirt/base_color.png"),
    ));
}

fn wait_for_assets_loaded(
    mut commands: Commands,
    loading_texture: Res<LoadingTexture>,
    mut array_materials: ResMut<Assets<ArrayMaterial>>,
    mut textures: ResMut<Assets<Texture>>,
) {
    if let Some(texture) = textures.get_mut(&loading_texture.0) {
        println!("Done loading mesh texture");
        prepare_materials_texture(texture);
        let mut material = ArrayMaterial::from(loading_texture.0.clone());
        material.roughness = 0.8;
        material.reflectance = 0.2;
        commands.insert_resource(MeshMaterial(array_materials.add(material)));
    }
}

fn prepare_materials_texture(texture: &mut Texture) {
    let num_layers = 4;
    texture.reinterpret_stacked_2d_as_array(num_layers);
    texture.sampler.address_mode_u = AddressMode::Repeat;
    texture.sampler.address_mode_v = AddressMode::Repeat;
}
