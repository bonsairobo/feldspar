use slab::Slab;
use static_assertions::const_assert_eq;
use std::{
    mem,
    num::NonZeroU32,
    ops::{Index, IndexMut},
};

/// An opaque number that uniquely identifies the value stored in a given [`Allocator32`].
///
/// An `Option<AllocId32>` still only requires 32 bits.
pub type AllocId32 = NonZeroU32;

const_assert_eq!(
    mem::size_of::<Option<AllocId32>>(),
    mem::size_of::<AllocId32>()
);

/// Stores up to `u32::MAX` values of type `T`. Indexed by 32-bit [`AllocId32`].
pub struct Allocator32<T> {
    values: Slab<T>,
}

impl<T> Allocator32<T> {
    #[inline]
    pub unsafe fn get_unchecked(&self, id: AllocId32) -> &T {
        self.values.get_unchecked(Self::id_to_index(id))
    }

    #[inline]
    pub unsafe fn get_unchecked_mut(&mut self, id: AllocId32) -> &mut T {
        self.values.get_unchecked_mut(Self::id_to_index(id))
    }

    #[inline]
    pub fn get(&self, id: AllocId32) -> Option<&T> {
        self.values.get(Self::id_to_index(id))
    }

    #[inline]
    pub fn get_mut(&mut self, id: AllocId32) -> Option<&mut T> {
        self.values.get_mut(Self::id_to_index(id))
    }

    #[inline]
    pub fn insert(&mut self, value: T) -> AllocId32 {
        let index = self.values.insert(value);
        Self::index_to_id(index)
    }

    #[inline]
    pub fn remove(&mut self, id: AllocId32) -> T {
        self.values.remove(Self::id_to_index(id))
    }

    const MAX_VALID_INDEX: usize = (u32::MAX - 1) as usize;

    const fn id_to_index(id: AllocId32) -> usize {
        // XOR is used to flip all of the bits of id so that u32::MAX is mapped to zero (a valid slab index).
        (id.get() ^ u32::MAX) as usize
    }

    fn index_to_id(index: usize) -> AllocId32 {
        assert!(index <= Self::MAX_VALID_INDEX);
        unsafe { Self::index_to_id_unchecked(index) }
    }

    /// `index` must be less than `u32::MAX`.
    const unsafe fn index_to_id_unchecked(index: usize) -> AllocId32 {
        // XOR is used to flip all of the bits of id so that u32::MAX is mapped to zero (a valid slab index).
        AllocId32::new_unchecked((index as u32) ^ u32::MAX)
    }
}

impl<C> Index<AllocId32> for Allocator32<C> {
    type Output = C;

    #[inline]
    fn index(&self, id: AllocId32) -> &Self::Output {
        self.values.index(Self::id_to_index(id))
    }
}

impl<C> IndexMut<AllocId32> for Allocator32<C> {
    #[inline]
    fn index_mut(&mut self, id: AllocId32) -> &mut Self::Output {
        self.values.index_mut(Self::id_to_index(id))
    }
}
