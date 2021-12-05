use glam::{IVec2, IVec3};
use ndshape::ConstShape;
use std::marker::PhantomData;
use std::ops::{Index, IndexMut};

/// An N-dimensional array view. Use [`Index`] and [`IndexMut`] to access values with `[i32; N]` coordinates.
pub struct NdView<Data, S> {
    pub values: Data,
    shape: PhantomData<S>,
}

impl<Data, S> NdView<Data, S> {
    #[inline]
    pub fn new(values: Data) -> Self {
        Self {
            values,
            shape: PhantomData,
        }
    }
}

impl<Data, S, const N: usize> Index<[i32; N]> for NdView<Data, S>
where
    Data: Index<usize>,
    S: ConstShape<i32, N>,
{
    type Output = Data::Output;

    #[inline]
    fn index(&self, index: [i32; N]) -> &Self::Output {
        self.values.index(S::linearize(index) as usize)
    }
}

impl<Data, S, const N: usize> IndexMut<[i32; N]> for NdView<Data, S>
where
    Data: IndexMut<usize>,
    S: ConstShape<i32, N>,
{
    #[inline]
    fn index_mut(&mut self, index: [i32; N]) -> &mut Self::Output {
        self.values.index_mut(S::linearize(index) as usize)
    }
}

impl<Data, S> Index<IVec2> for NdView<Data, S>
where
    Data: Index<usize>,
    S: ConstShape<i32, 2>,
{
    type Output = Data::Output;

    #[inline]
    fn index(&self, index: IVec2) -> &Self::Output {
        self.values.index(S::linearize(index.to_array()) as usize)
    }
}

impl<Data, S> Index<IVec3> for NdView<Data, S>
where
    Data: Index<usize>,
    S: ConstShape<i32, 3>,
{
    type Output = Data::Output;

    #[inline]
    fn index(&self, index: IVec3) -> &Self::Output {
        self.values.index(S::linearize(index.to_array()) as usize)
    }
}
