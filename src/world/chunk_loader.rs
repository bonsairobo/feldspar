use crate::{Config, VoxelEditor, VoxelWorldDb, Witness};

use bevy::prelude::*;

pub fn chunk_loader_system(
    config: Res<Config>,
    db: Res<VoxelWorldDb>,
    mut editor: VoxelEditor,
    witnesses: Query<(&Witness, &Transform)>,
) {
    for (_witness, tfm) in witnesses.iter() {}
}
