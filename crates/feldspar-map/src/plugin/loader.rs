use super::Witness;
use crate::clipmap::ChunkClipMap;

use bevy::prelude::*;

pub fn loader_system(clipmap: Res<ChunkClipMap>, witnesses: Query<(&Witness, &Transform)>) {}
