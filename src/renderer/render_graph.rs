mod voxel_pipeline;

pub use voxel_pipeline::*;

use crate::ArrayMaterial;

use bevy::ecs::world::World;

pub mod node {
    pub const ARRAY_MATERIAL: &str = "array_material";
}

use bevy::asset::Assets;
use bevy::render::{
    pipeline::PipelineDescriptor,
    render_graph::{base, AssetRenderResourcesNode, RenderGraph},
    shader::Shader,
};

pub(crate) fn add_voxel_render_graph(world: &mut World) {
    {
        let mut graph = world.get_resource_mut::<RenderGraph>().unwrap();
        graph.add_system_node(
            node::ARRAY_MATERIAL,
            AssetRenderResourcesNode::<ArrayMaterial>::new(true),
        );
        graph
            .add_node_edge(node::ARRAY_MATERIAL, base::node::MAIN_PASS)
            .unwrap();
    }
    let pipeline = build_pbr_pipeline(&mut world.get_resource_mut::<Assets<Shader>>().unwrap());
    let mut pipelines = world
        .get_resource_mut::<Assets<PipelineDescriptor>>()
        .unwrap();
    pipelines.set_untracked(PBR_PIPELINE_HANDLE, pipeline);
}
