use ilattice::glam::{IVec2, IVec3};
use ndshape::Shape;
use std::marker::PhantomData;
use std::ops::{Index, IndexMut};

/// An N-dimensional array view. Use [`Index`] and [`IndexMut`] to access values with `[i32; N]` coordinates.
pub struct NdView<T, Data, S> {
    pub values: Data,
    shape: S,
    marker: PhantomData<T>,
}

impl<T, Data, S> NdView<T, Data, S> {
    #[inline]
    pub fn new(values: Data, shape: S) -> Self {
        Self {
            values,
            shape,
            marker: PhantomData,
        }
    }
}

impl<T, Data, S, const N: usize> Index<[i32; N]> for NdView<T, Data, S>
where
    Data: AsRef<[T]>,
    S: Shape<i32, N>,
{
    type Output = T;

    #[inline]
    fn index(&self, index: [i32; N]) -> &Self::Output {
        self.values
            .as_ref()
            .index(self.shape.linearize(index) as usize)
    }
}

impl<T, Data, S, const N: usize> IndexMut<[i32; N]> for NdView<T, Data, S>
where
    Data: AsRef<[T]> + AsMut<[T]>,
    S: Shape<i32, N>,
{
    #[inline]
    fn index_mut(&mut self, index: [i32; N]) -> &mut Self::Output {
        self.values
            .as_mut()
            .index_mut(self.shape.linearize(index) as usize)
    }
}

impl<T, Data, S> Index<IVec2> for NdView<T, Data, S>
where
    Data: AsRef<[T]>,
    S: Shape<i32, 2>,
{
    type Output = T;

    #[inline]
    fn index(&self, index: IVec2) -> &Self::Output {
        self.values
            .as_ref()
            .index(self.shape.linearize(index.to_array()) as usize)
    }
}

impl<T, Data, S> IndexMut<IVec2> for NdView<T, Data, S>
where
    Data: AsRef<[T]> + AsMut<[T]>,
    S: Shape<i32, 2>,
{
    #[inline]
    fn index_mut(&mut self, index: IVec2) -> &mut Self::Output {
        self.values
            .as_mut()
            .index_mut(self.shape.linearize(index.to_array()) as usize)
    }
}

impl<T, Data, S> Index<IVec3> for NdView<T, Data, S>
where
    Data: AsRef<[T]>,
    S: Shape<i32, 3>,
{
    type Output = T;

    #[inline]
    fn index(&self, index: IVec3) -> &Self::Output {
        self.values
            .as_ref()
            .index(self.shape.linearize(index.to_array()) as usize)
    }
}

impl<T, Data, S> IndexMut<IVec3> for NdView<T, Data, S>
where
    Data: AsRef<[T]> + AsMut<[T]>,
    S: Shape<i32, 3>,
{
    #[inline]
    fn index_mut(&mut self, index: IVec3) -> &mut Self::Output {
        self.values
            .as_mut()
            .index_mut(self.shape.linearize(index.to_array()) as usize)
    }
}
