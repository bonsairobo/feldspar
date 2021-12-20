use crate::{
    bitset::{AtomicBitset8, Bitset8},
    chunk::{Chunk, CompressedChunk},
};

use either::Either;
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use static_assertions::const_assert_eq;
use std::mem::{self, ManuallyDrop};
use std::sync::atomic::Ordering;

/// A single node in the [`ChunkClipMap`](crate::ChunkClipMap).
///
/// Optimized for separate read and write phases in each frame. Voxel editors will just be readers that also write out of place
/// and merge their changes into the map in the write phase, which requires exclusive `&mut` access.
///
/// While the chunk is compressed, readers will take an exclusive lock and wait for one of the readers to decompress the chunk
/// before continuing. Decompression should happen at most once per frame.
#[derive(Default)]
pub struct ChunkNode {
    chunk: RwLock<ChunkSlot>,
    state: NodeState,
}

const_assert_eq!(mem::size_of::<ChunkNode>(), 4 * mem::size_of::<usize>());

impl Drop for ChunkNode {
    fn drop(&mut self) {
        self.replace_slot(ChunkSlot { empty: () });
    }
}

impl ChunkNode {
    #[inline]
    pub fn state(&self) -> &NodeState {
        &self.state
    }

    pub fn new_empty(state: NodeState) -> Self {
        state.state.unset_bit(StateBit::Occupied as u8);
        Self {
            state,
            chunk: RwLock::new(ChunkSlot { empty: () }),
        }
    }

    pub fn new_compressed(chunk: CompressedChunk, state: NodeState) -> Self {
        state.state.set_bit(StateBit::Occupied as u8);
        state.state.set_bit(StateBit::Compressed as u8);
        Self {
            state,
            chunk: RwLock::new(ChunkSlot {
                compressed: ManuallyDrop::new(chunk),
            }),
        }
    }

    pub fn new_decompressed(chunk: Box<Chunk>, state: NodeState) -> Self {
        state.state.set_bit(StateBit::Occupied as u8);
        state.state.unset_bit(StateBit::Compressed as u8);
        Self {
            state,
            chunk: RwLock::new(ChunkSlot {
                decompressed: ManuallyDrop::new(chunk),
            }),
        }
    }

    /// If the slot is currently compressed, then the compressed value is dropped.
    pub fn get_decompressed(&self) -> Option<DecompressedChunk<'_>> {
        match self.state.slot_state() {
            SlotState::Compressed => self.decompress_for_read(),
            SlotState::Decompressed => {
                // Fast path for when the chunk is already decompressed.
                Some(DecompressedChunk {
                    read_guard: self.chunk.read(),
                })
            }
            SlotState::Empty => None,
        }
    }

    #[cold]
    fn decompress_for_read(&self) -> Option<DecompressedChunk<'_>> {
        let mut write_guard = self.chunk.write();

        match self.state.slot_state() {
            SlotState::Compressed => {
                // We are the lucky thread that gets to do inline decompression! Other threads are waiting for us to decompress
                // and drop the exclusive lock.

                // Decompress the chunk inline.
                let decompressed = Box::new(unsafe { &write_guard.compressed }.decompress());
                unsafe { ManuallyDrop::drop(&mut write_guard.compressed) };
                write_guard.decompressed = ManuallyDrop::new(decompressed);

                // Readers don't need to wait anymore.
                self.state.state.unset_bit(StateBit::Compressed as u8);

                Some(DecompressedChunk {
                    read_guard: RwLockWriteGuard::downgrade(write_guard),
                })
            }
            SlotState::Decompressed => {
                // Some other thread already decompressed for us. Downgrade to a read lock.
                Some(DecompressedChunk {
                    read_guard: RwLockWriteGuard::downgrade(write_guard),
                })
            }
            SlotState::Empty => None,
        }
    }

    /// Replace the existing chunk value with a [`CompressedChunk`].
    pub fn put_compressed(
        &mut self,
        compressed: CompressedChunk,
    ) -> Option<Either<Box<Chunk>, CompressedChunk>> {
        let old_value = self.replace_slot(ChunkSlot {
            compressed: ManuallyDrop::new(compressed),
        });
        self.state.state.set_bit(StateBit::Occupied as u8);
        self.state.state.set_bit(StateBit::Compressed as u8);
        old_value
    }

    /// Replace the existing chunk value with a [`Box<Chunk>`].
    pub fn put_decompressed(
        &mut self,
        decompressed: Box<Chunk>,
    ) -> Option<Either<Box<Chunk>, CompressedChunk>> {
        let old_value = self.replace_slot(ChunkSlot {
            decompressed: ManuallyDrop::new(decompressed),
        });
        self.state.state.set_bit(StateBit::Occupied as u8);
        self.state.state.unset_bit(StateBit::Compressed as u8);
        old_value
    }

    /// Take the existing chunk value, leaving the slot empty.
    pub fn take_chunk(&mut self) -> Option<Either<Box<Chunk>, CompressedChunk>> {
        let old_value = self.replace_slot(ChunkSlot { empty: () });
        self.state.state.unset_bit(StateBit::Occupied as u8);
        old_value
    }

    fn replace_slot(&mut self, new_slot: ChunkSlot) -> Option<Either<Box<Chunk>, CompressedChunk>> {
        let mut_slot = self.chunk.get_mut();
        match self.state.slot_state() {
            SlotState::Compressed => Some(Either::Right(ManuallyDrop::into_inner(unsafe {
                mem::replace(&mut *mut_slot, new_slot).compressed
            }))),
            SlotState::Decompressed => Some(Either::Left(ManuallyDrop::into_inner(unsafe {
                mem::replace(&mut *mut_slot, new_slot).decompressed
            }))),
            SlotState::Empty => {
                drop(mem::replace(&mut *mut_slot, new_slot));
                None
            }
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
enum StateBit {
    /// This bit is set if there is chunk data in the slot.
    Occupied = 0,
    /// This bit is set if the node is compressed or in the process of being decompressed.
    Compressed = 1,
    /// This bit is set if the node is currently loading.
    Loading = 2,
    /// This bit is set if the node is currently being rendered.
    Render = 3,
}

impl StateBit {
    const fn mask(&self) -> u8 {
        1 << *self as u8
    }
}

const OCCUPIED_MASK: u8 = StateBit::Occupied.mask();
const COMPRESSED_MASK: u8 = StateBit::Compressed.mask();

#[derive(Default)]
pub struct NodeState {
    pub(crate) descendant_is_loading: Bitset8,
    pub(crate) state: AtomicBitset8,
}

impl NodeState {
    #[inline]
    pub fn slot_state(&self) -> SlotState {
        const MASK: u8 = OCCUPIED_MASK | COMPRESSED_MASK;
        let and_mask = self.state.bits.fetch_and(MASK, Ordering::SeqCst);
        match (
            and_mask & OCCUPIED_MASK != 0,
            and_mask & COMPRESSED_MASK != 0,
        ) {
            (true, true) => SlotState::Compressed,
            (true, false) => SlotState::Decompressed,
            (false, _) => SlotState::Empty,
        }
    }

    #[inline]
    pub fn is_loading(&self) -> bool {
        self.state.bit_is_set(StateBit::Loading as u8)
    }

    #[inline]
    pub fn tree_is_loading(&self) -> bool {
        self.is_loading() || self.descendant_is_loading.any()
    }

    #[inline]
    pub fn is_rendering(&self) -> bool {
        self.state.bit_is_set(StateBit::Render as u8)
    }

    #[inline]
    fn mark_loaded(&self) -> bool {
        self.state.fetch_and_unset_bit(StateBit::Loading as u8)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlotState {
    Empty,
    Compressed,
    Decompressed,
}

/// A safe wrapper around a [`Chunk`] protected by an [`RwLockReadGuard`].
pub struct DecompressedChunk<'a> {
    read_guard: RwLockReadGuard<'a, ChunkSlot>,
}

impl<'a> AsRef<Chunk> for DecompressedChunk<'a> {
    fn as_ref(&self) -> &Chunk {
        // SAFE: Internals of ChunkNode guarantee this is a decompressed chunk.
        unsafe { self.read_guard.decompressed.as_ref() }
    }
}

/// This slot type is nearly equivalent to this enum:
/// ```
/// # use feldspar_map::chunk::{Chunk, CompressedChunk};
/// enum ChunkSlot {
///     Empty,
///     Compressed(CompressedChunk),
///     Decompressed(Box<Chunk>),
/// }
/// ```
/// except that its discriminant lives on the [`ChunkNode`] that owns it, and fields must be manually dropped based on that
/// discriminant.
union ChunkSlot {
    empty: (),
    compressed: mem::ManuallyDrop<CompressedChunk>,
    decompressed: mem::ManuallyDrop<Box<Chunk>>,
}

impl Default for ChunkSlot {
    fn default() -> Self {
        Self { empty: () }
    }
}

const_assert_eq!(
    mem::size_of::<ChunkSlot>(),
    2 * mem::size_of::<*const i32>()
);

// ████████╗███████╗███████╗████████╗
// ╚══██╔══╝██╔════╝██╔════╝╚══██╔══╝
//    ██║   █████╗  ███████╗   ██║
//    ██║   ██╔══╝  ╚════██║   ██║
//    ██║   ███████╗███████║   ██║
//    ╚═╝   ╚══════╝╚══════╝   ╚═╝

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn chunk_node_data_slot_round_trip() {
        let compressed_chunk = Chunk::default().compress();

        let mut node = ChunkNode::new_empty(NodeState::default());

        let chunk = Box::new(Chunk::default());
        let old = node.put_decompressed(chunk);
        assert_eq!(old, None);
        assert_eq!(node.state().slot_state(), SlotState::Decompressed);

        node.put_compressed(compressed_chunk.clone());
        assert_eq!(node.state().slot_state(), SlotState::Compressed);

        // Spawn some threads to read the chunk. All of them should see the same value, and it should only be decompressed once.
        let node_ref = &node;
        crossbeam::scope(move |scope| {
            for _ in 0..10 {
                scope.spawn(|_| {
                    let decompressed = node_ref.get_decompressed().unwrap();
                    assert_eq!(decompressed.as_ref(), &Chunk::default());
                });
            }
        })
        .unwrap();
        assert_eq!(node.state().slot_state(), SlotState::Decompressed);

        node.put_compressed(compressed_chunk.clone());
        assert_eq!(node.state().slot_state(), SlotState::Compressed);

        let chunk = node.take_chunk();
        assert_eq!(chunk, Some(Either::Right(compressed_chunk.clone())));
        assert_eq!(node.state().slot_state(), SlotState::Empty);
    }
}
