use crate::clipmap::Level;
use crate::core::glam::IVec3;
use crate::core::ilattice::prelude::{Bounded, Extent, Morton3i32};
use crate::core::rkyv::{Archive, Deserialize, Serialize};

use core::ops::RangeInclusive;

#[derive(
    Archive, Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, PartialOrd, Ord, Serialize,
)]
#[archive(crate = "crate::core::rkyv")]
#[archive_attr(derive(Debug, Eq, Hash, PartialEq, PartialOrd, Ord))]
pub struct ChunkDbKey {
    pub level: Level,
    pub morton: Morton3i32,
}

impl ChunkDbKey {
    pub fn new(level: Level, morton: Morton3i32) -> Self {
        Self { level, morton }
    }

    /// We implement this manually (without rkyv) so we have control over the [`Ord`] as interpreted by [`sled`].
    ///
    /// 13 bytes total per key, 1 for LOD and 12 for the morton code. Although a [`Morton3i32`] uses a u128, it only actually
    /// uses the least significant 96 bits (12 bytes).
    pub fn into_sled_key(&self) -> [u8; 13] {
        let mut bytes = [0; 13];
        bytes[0] = self.level;
        bytes[1..].copy_from_slice(&self.morton.0.to_be_bytes()[4..]);
        bytes
    }

    pub fn from_sled_key(bytes: &[u8]) -> Self {
        let level = bytes[0];
        // The most significant 4 bytes of the u128 are not used.
        let mut morton_bytes = [0; 16];
        morton_bytes[4..16].copy_from_slice(&bytes[1..]);
        let morton_int = u128::from_be_bytes(morton_bytes);
        Self::new(level, Morton3i32(morton_int))
    }

    pub fn extent_range(level: u8, extent: Extent<IVec3>) -> RangeInclusive<Self> {
        let min_morton = Morton3i32::from(extent.minimum);
        let max_morton = Morton3i32::from(extent.max());
        Self::new(level, min_morton)..=Self::new(level, max_morton)
    }

    pub fn min_key(level: u8) -> Self {
        Self::new(level, Morton3i32::from(IVec3::MIN))
    }

    pub fn max_key(level: u8) -> Self {
        Self::new(level, Morton3i32::from(IVec3::MAX))
    }
}
