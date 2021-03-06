use crate::clipmap::ChildIndex;

/// Represents 2^3 neighborhood subdivisions.
///
/// This constant is generated by the "generate-kernels" binary target in this crate.
///
/// This example is in 2D for readability, but we need to do it in 3D.
///
/// ```text
/// children             4 neighborhoods
///
///       |
/// 00 01 | 00           [00, 01, 10, 11]
/// ------+------  --->  [01, 00, 11, 10]
/// 10 11 | 10           [10, 11, 00, 01]
/// 00 01 | 00           [11, 10, 01, 00]
/// ```
pub const NEIGHBORHOODS: [[ChildIndex; 8]; 8] = [
    [0b000, 0b001, 0b010, 0b011, 0b100, 0b101, 0b110, 0b111],
    [0b001, 0b000, 0b011, 0b010, 0b101, 0b100, 0b111, 0b110],
    [0b010, 0b011, 0b000, 0b001, 0b110, 0b111, 0b100, 0b101],
    [0b011, 0b010, 0b001, 0b000, 0b111, 0b110, 0b101, 0b100],
    [0b100, 0b101, 0b110, 0b111, 0b000, 0b001, 0b010, 0b011],
    [0b101, 0b100, 0b111, 0b110, 0b001, 0b000, 0b011, 0b010],
    [0b110, 0b111, 0b100, 0b101, 0b010, 0b011, 0b000, 0b001],
    [0b111, 0b110, 0b101, 0b100, 0b011, 0b010, 0b001, 0b000],
];

/// Represents 2^3 neighborhood subdivisions.
///
/// This constant is generated by the "generate-kernels" binary target in this crate.
///
/// This example is in 2D for readability, but we need to do it in 3D.
///
/// ```text
/// parents              4 neighborhoods
///
///       |
/// 10 10 | 11           [00, 00, 00, 00]
/// ------+------  --->  [00, 01, 00, 01]
/// 00 00 | 01           [00, 00, 10, 10]
/// 00 00 | 01           [00, 01, 10, 11]
/// ```
pub const NEIGHBORHOODS_PARENTS: [[ChildIndex; 8]; 8] = [
    [0b000, 0b000, 0b000, 0b000, 0b000, 0b000, 0b000, 0b000],
    [0b000, 0b001, 0b000, 0b001, 0b000, 0b001, 0b000, 0b001],
    [0b000, 0b000, 0b010, 0b010, 0b000, 0b000, 0b010, 0b010],
    [0b000, 0b001, 0b010, 0b011, 0b000, 0b001, 0b010, 0b011],
    [0b000, 0b000, 0b000, 0b000, 0b100, 0b100, 0b100, 0b100],
    [0b000, 0b001, 0b000, 0b001, 0b100, 0b101, 0b100, 0b101],
    [0b000, 0b000, 0b010, 0b010, 0b100, 0b100, 0b110, 0b110],
    [0b000, 0b001, 0b010, 0b011, 0b100, 0b101, 0b110, 0b111],
];

#[cfg(test)]
mod tests {
    use super::*;

    use crate::coordinates::CUBE_CORNERS;
    use crate::core::glam::IVec3;

    use ndshape::{ConstPow2Shape3i32, ConstShape};

    use std::fmt::Debug;

    #[test]
    fn constants_are_generated() {
        let [neighborhoods, neighborhoods_parents] = generate_neighborhoods();

        println!(
            "neighborhoods = {:?}",
            neighborhoods.map(|n| n.map(|x| Binary3(x)))
        );
        println!(
            "neighborhoods_parents = {:?}",
            neighborhoods_parents.map(|n| n.map(|x| Binary3(x)))
        );

        assert_eq!(neighborhoods, NEIGHBORHOODS);
        assert_eq!(neighborhoods_parents, NEIGHBORHOODS_PARENTS);
    }

    /// Used to format binary triplets e.g. `0b111`.
    #[derive(Clone, Copy)]
    pub struct Binary3(u8);

    impl Debug for Binary3 {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:#05b}", self.0)
        }
    }

    /// Generates [`NEIGHBORHOODS`] and [`NEIGHBORHOODS_PARENTS`].
    fn generate_neighborhoods() -> [[[u8; 8]; 8]; 2] {
        type HalfShape = ConstPow2Shape3i32<1, 1, 1>;
        type FullShape = ConstPow2Shape3i32<2, 2, 2>;

        let mut grid = [0; FullShape::SIZE as usize];
        for &o1 in CUBE_CORNERS.iter() {
            let min = 2 * o1;
            for &o2 in CUBE_CORNERS.iter() {
                let pos = min + o2;
                let pos_i = FullShape::linearize(pos.to_array());
                grid[pos_i as usize] = HalfShape::linearize(o2.to_array()) as u8;
            }
        }

        let mut neighborhoods = [[0; 8]; 8];
        let mut neighborhoods_parents = [[0; 8]; 8];
        for &min in CUBE_CORNERS.iter() {
            let i1 = HalfShape::linearize(min.to_array());
            for &offset in CUBE_CORNERS.iter() {
                let i2 = HalfShape::linearize(offset.to_array());
                let pos = min + offset;
                let parent: IVec3 = pos >> 1;
                let parent_i = HalfShape::linearize(parent.to_array());
                let pos_i = FullShape::linearize(pos.to_array());
                neighborhoods[i1 as usize][i2 as usize] = grid[pos_i as usize];
                neighborhoods_parents[i1 as usize][i2 as usize] = parent_i as u8;
            }
        }

        [neighborhoods, neighborhoods_parents]
    }
}
