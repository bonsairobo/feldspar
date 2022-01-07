mod loader;
mod witness;

pub use witness::Witness;

use bevy::prelude::Plugin;

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {}
}
