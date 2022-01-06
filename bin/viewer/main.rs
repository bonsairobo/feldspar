use bevy::{
    pbr::wireframe::{WireframeConfig, WireframePlugin},
    prelude::*,
    render::{options::WgpuOptions, render_resource::WgpuFeatures},
};
use feldspar_map::MapPlugin;
use feldspar_renderer::RenderPlugin;
use smooth_bevy_cameras::{
    controllers::fps::{FpsCameraBundle, FpsCameraController, FpsCameraPlugin},
    LookTransformPlugin,
};

fn main() {
    let window_desc = WindowDescriptor {
        width: 1600.0,
        height: 900.0,
        title: "Feldspar Map Viewer".to_string(),
        ..Default::default()
    };

    App::new()
        .insert_resource(window_desc)
        .insert_resource(WgpuOptions {
            features: WgpuFeatures::POLYGON_MODE_LINE,
            ..Default::default()
        })
        .insert_resource(Msaa { samples: 4 })
        // Bevy,
        .add_plugins(DefaultPlugins)
        .add_plugin(WireframePlugin)
        // Feldspar.
        .add_plugin(MapPlugin)
        .add_plugin(RenderPlugin)
        // Viewer.
        .add_plugin(LookTransformPlugin)
        .add_plugin(FpsCameraPlugin::default())
        .add_startup_system(setup.system())
        .run();
}

fn setup(mut commands: Commands, mut wireframe_config: ResMut<WireframeConfig>) {
    wireframe_config.global = true;

    commands.spawn_bundle(PointLightBundle {
        transform: Transform::from_translation(Vec3::new(25.0, 25.0, 25.0)),
        point_light: PointLight {
            range: 200.0,
            intensity: 8000.0,
            ..Default::default()
        },
        ..Default::default()
    });
    commands.spawn_bundle(PerspectiveCameraBundle {
        transform: Transform::from_translation(Vec3::new(50.0, 15.0, 50.0))
            .looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y),
        ..Default::default()
    });
}
