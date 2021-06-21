//! The Feldspar voxel plugin for Bevy Engine.

mod assets;
mod bvt;
mod config;
mod database;
mod renderer;
mod thread_local_resource;
mod voxel_data;
mod world;

use bvt::*;
use renderer::*;
use thread_local_resource::*;
use voxel_data::*;

pub use bvt::VoxelBvt;
pub use config::Config;
pub use database::VoxelWorldDb;
pub use voxel_data::{SdfVoxelMap, VoxelEditor};
pub use world::VoxelWorldPlugin;

use bevy::ecs::component::Component;
use std::fmt::Debug;
use std::hash::Hash;

pub trait BevyState: Clone + Component + Debug + Eq + Hash {}

impl<T> BevyState for T where T: Clone + Component + Debug + Eq + Hash {}
