//! The feldspar map data model.
//!
//! # Voxels
//!
//! Voxels are used to describe semi-sparse volumetric data in a map. [`Chunk`](crate::Chunk)s of voxels are allocated at a
//! time, but not all partitions of space are occupied by chunk data. Empty partitions are assumed to have some constant
//! "ambient value."
//!
//! ## Terrain Voxels
//!
//! A signed distance field (SDF) determines the terrain geometry. The maximum distance value (one voxel edge length) at LOD0 is
//! approximately 1 meter. SDF values ([`Sd8`](crate::Sd8)) have 8-bit precision at all LODs. This implies that the minimum
//! signed distance value at LOD0 is `1 / 2^8` meters. SDF voxels can be downsampled for LOD purposes. LZ4 compression is
//! effective on SDF voxel chunks.
//!
//! ## Material Voxels
//!
//! A voxel's [`PaletteId8`](crate::PaletteId8) is used to look up arbitrary attributes about a voxel via a `Palette8`. Only 256
//! materials are supported in a single map. The attributes often consist of textures and physical properties like chemical
//! makeup.
//!
//! ## Tile Voxels
//!
//! During the process of procedural generation, it can be useful to think of entire chunks as "tiles." In this way, data can be
//! shared between multiple instances of a tile. When a chunk is edited, it needs to copy the original tile's chunk before
//! modification. This introduces another layer of indirection for reads as well.

mod allocator;
mod bitset;
mod chunk;
mod clipmap;
mod database;
mod geometry;
mod ndview;
mod palette;
mod sdf;

pub use allocator::*;
pub use chunk::*;
pub use clipmap::*;
pub use database::*;
pub use geometry::*;
pub use ndview::*;
pub use palette::*;
pub use sdf::*;
