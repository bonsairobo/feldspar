use std::ops::{Index, IndexMut};

/// An identifier for one of the values in a given [`Palette8`].
pub type PaletteId8 = u8;

/// A mapping from [`PaletteId8`] to type `T`. This can store up to 256 values.
///
/// Use [`Index`] and [`IndexMut`] traits for access.
#[derive(Clone, Debug)]
pub struct Palette8<T> {
    types: Vec<T>,
}

impl<T> Index<PaletteId8> for Palette8<T> {
    type Output = T;

    #[inline]
    fn index(&self, id: PaletteId8) -> &Self::Output {
        self.types.index(id as usize)
    }
}

impl<T> IndexMut<PaletteId8> for Palette8<T> {
    #[inline]
    fn index_mut(&mut self, id: PaletteId8) -> &mut Self::Output {
        self.types.index_mut(id as usize)
    }
}
