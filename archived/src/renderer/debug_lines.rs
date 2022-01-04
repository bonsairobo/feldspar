use crate::prelude::{witness_superchunk_extent, Config, Witness};

use building_blocks::core::{prelude::*, EDGES_3};

use bevy::prelude::*;
use bevy::render::color::Color;
use bevy_prototype_debug_lines::DebugLines;
use std::collections::HashSet;
use std::iter::FromIterator;

pub fn insert_extent_boundary_lines(extent: Extent3f, color: Color, lines: &mut DebugLines) {
    let corners = extent.corners();

    let mut boundary_lines = [[Vec3::ZERO, Vec3::ZERO]; 12];
    for i in 0..12 {
        boundary_lines[i] = [
            Vec3::from(corners[EDGES_3[i][0]]),
            Vec3::from(corners[EDGES_3[i][1]]),
        ];
    }

    for &[start, end] in boundary_lines.iter() {
        lines.line_colored(start, end, 0.0, color);
    }
}

pub fn debug_chunk_boundaries_system(
    config: Res<Config>,
    witnesses: Query<(&Witness, &Transform)>,
    mut debug_lines: ResMut<DebugLines>,
) {
    for (_witness, tfm) in witnesses.iter() {
        let center = Point3f::from(tfm.translation).in_voxel();

        let witness_extent = witness_superchunk_extent(
            center,
            config.witness_radius,
            config.map.superchunk_exponent,
        );

        let superchunks = HashSet::<Point3i>::from_iter(witness_extent.0.iter_points());
        for superchunk in superchunks.into_iter() {
            let octant = Octant::new(config.map.superchunk_exponent as i32, superchunk);
            let extent = Extent3i::from(octant);
            let extentf = Extent3f::from(extent);
            insert_extent_boundary_lines(extentf, Color::GREEN, &mut debug_lines);
        }
    }
}
