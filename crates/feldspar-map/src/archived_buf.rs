use rkyv::{archived_root, Archive, Archived, Deserialize, Infallible};
use std::marker::PhantomData;

/// A wrapper around a byte buffer `B` that denotes the bytes represent an [`Archived<T>`].
///
/// Note: While not unsafe, this is not intended to be used with structures that utilize shared memory like `Rc` and `Arc`.
#[derive(Clone, Eq, PartialEq)]
pub struct ArchivedBuf<T, B> {
    bytes: B,
    marker: PhantomData<T>,
}

impl<T, B> ArchivedBuf<T, B>
where
    T: Archive,
    B: AsRef<[u8]>,
{
    /// # Safety
    ///
    /// - `bytes` must faithfully represent an [`Archived<T>`]
    /// - the same constraints apply as if you were calling [`archived_root`] on `bytes`
    pub unsafe fn new(bytes: B) -> Self {
        Self {
            bytes,
            marker: PhantomData,
        }
    }

    pub fn deserialize(&self) -> T
    where
        T::Archived: Deserialize<T, Infallible>,
    {
        self.as_ref().deserialize(&mut Infallible).unwrap()
    }

    pub fn take_bytes(self) -> B {
        self.bytes
    }
}

impl<T, B> AsRef<Archived<T>> for ArchivedBuf<T, B>
where
    T: Archive,
    B: AsRef<[u8]>,
{
    fn as_ref(&self) -> &Archived<T> {
        unsafe { archived_root::<T>(self.bytes.as_ref()) }
    }
}
