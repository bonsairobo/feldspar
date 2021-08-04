//! The Feldspar voxel plugin for Bevy Engine.

mod config;

pub mod bvt;
pub mod renderer;
pub mod thread_local_resource;
pub mod voxel_data;
pub mod world;

pub use config::Config;

pub use building_blocks as bb;

pub mod prelude {
    pub use super::bvt::*;
    pub use super::config::*;
    pub use super::renderer::*;
    pub use super::thread_local_resource::*;
    pub use super::voxel_data::*;
    pub use super::world::*;
}

use bevy::ecs::component::Component;
use std::fmt::Debug;
use std::hash::Hash;

pub trait BevyState: Clone + Component + Debug + Eq + Hash {}

impl<T> BevyState for T where T: Clone + Component + Debug + Eq + Hash {}
