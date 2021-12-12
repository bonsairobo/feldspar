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
//!
//! # Database
//!
//! The [`MapDatabase`] provides versioned, persistent storage for all map data, mostly [`Chunk`]s. Each non-current version
//! only stores the deltas required to reach that version from a neighboring version. By the structure of the version tree and
//! transitivity, every version is reachable from the current one. Transitioning between versions can be done by either:
//!
//! - creating a new version by flushing outstanding edits
//! - forcing a transition along the full path from the current version to the destination version
//!
//! # Multiresolution Streaming
//!
//! All voxel chunks in "observable range" are stored in the [`ChunkClipMap`] in either their raw or compressed representation.
//! [`Chunk`]s may also be downsampled to the appropriate resolution for rendering. The clipmap supports various queries that
//! iterate over a subset of the internal octree. These queries will contain relevant, high-priority items according to some
//! [`Ord`] implementation and a recursive predicate on the [`NodeState`]. This is used for:
//!
//! - finding chunks that should be loaded from the database
//! - finding chunks that should be downsampled
//! - finding chunks that should change their render detail
//! - finding empty chunks that can be dropped
//! - finding infrequently used chunks that should be compressed
//! - finding distant chunks that can be persisted and evicted from memory
//!
//! ## Chunk Slot State Machine (AKA Chunk State Slot Machine)
//!
//! ![chunk_fsm](/assets/chunk_fsm.png)
//!
//! # Bevy Plugin
//!
//! When building this crate with the `"bevy"` feature enabled, you get access to the [`MapPlugin`]. This Bevy `Plugin`
//! implements systems to drive the work of maintaining a [`ChunkClipMap`] and corresponding database. It exposes interfaces for
//! other Bevy ECS systems to both edit and query the currently loaded map without having to worry about the details of
//! streaming data and managing transactions.

mod allocator;
mod bitset;
mod chunk;
mod clipmap;
mod coordinates;
mod database;
mod geometry;
mod ndview;
mod palette;
mod sampling;
mod sdf;
mod units;

pub use allocator::*;
pub use chunk::*;
pub use clipmap::*;
pub use coordinates::*;
pub use database::*;
pub use geometry::*;
pub use ndview::*;
pub use palette::*;
pub use sdf::*;
pub use units::*;

#[cfg(feature = "bevy")]
mod plugin;
#[cfg(feature = "bevy")]
pub use plugin::*;

// Re-exports.
pub use ilattice;
pub use ilattice::glam;

use ahash::{AHashMap, AHashSet};

type SmallKeyHashMap<K, V> = AHashMap<K, V>;
type SmallKeyHashSet<K> = AHashSet<K>;
