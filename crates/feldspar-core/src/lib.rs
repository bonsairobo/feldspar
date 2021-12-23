pub mod allocator;
pub mod archived_buf;
pub mod bitset;
pub mod geometry;

use ahash::{AHashMap, AHashSet};
pub type SmallKeyHashMap<K, V> = AHashMap<K, V>;
pub type SmallKeyHashSet<K> = AHashSet<K>;

// Re-exports.
pub use approx;
pub use ilattice::glam as glam;
pub use ilattice;
pub use rkyv;
pub use static_assertions;

use rkyv::{
    ser::serializers::{
        AlignedSerializer, AllocScratch, CompositeSerializer, FallbackScratch, HeapScratch,
    },
    AlignedVec, Infallible,
};
pub type NoSharedAllocSerializer<const N: usize> = CompositeSerializer<
    AlignedSerializer<AlignedVec>,
    FallbackScratch<HeapScratch<N>, AllocScratch>,
    Infallible,
>;
