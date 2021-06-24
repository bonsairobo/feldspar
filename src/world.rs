mod chunk_loader;
mod plugin;

pub use chunk_loader::*;
pub use plugin::*;

use bevy::transform::components::Transform;

/// A component for any entity that needs to witness the voxel map, i.e. chunks should be loaded around it, and detail should be
/// highest near it. This only has any effect if the entity also has a `Transform`.
#[derive(Default)]
pub struct Witness {
    previous_transform: Option<Transform>,
}
