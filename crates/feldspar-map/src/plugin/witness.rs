use bevy::{prelude::Component, transform::components::Transform};

/// An entity (usually a camera) that gets a clip sphere in the clipmap.
#[derive(Component)]
pub struct Witness {
    pub(crate) previous_transform: Option<Transform>,
}
