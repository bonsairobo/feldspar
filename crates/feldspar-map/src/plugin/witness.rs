use bevy::prelude::*;

/// An entity (usually a camera) that gets a clip sphere in the clipmap.
#[derive(Component)]
pub struct Witness {
    pub(crate) previous_transform: Option<Transform>,
}

pub fn witness_system(mut witness_transforms: Query<(&mut Witness, &Transform)>) {
    for (mut witness, transform) in witness_transforms.iter_mut() {
        witness.previous_transform = Some(transform.clone());
    }
}
