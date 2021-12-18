use super::ChunkDbKey;
use crate::{CompressedChunk, SmallKeyHashMap};

use rkyv::{
    archived_root,
    ser::{
        serializers::{AllocSerializer, CoreSerializer},
        Serializer,
    },
    Archive, Serialize,
};
use sled::IVec;
use std::marker::PhantomData;

#[derive(Archive, Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum Change<T> {
    Insert(T),
    Remove,
}

impl<T> Change<T> {
    pub fn map<S>(self, mut f: impl FnMut(T) -> S) -> Change<S> {
        match self {
            Change::Insert(x) => Change::Insert(f(x)),
            Change::Remove => Change::Remove,
        }
    }

    pub fn serialize_to_ivec(&self) -> OwnedArchivedChange<T>
    where
        T: Serialize<AllocSerializer<8912>>,
    {
        let mut serializer = AllocSerializer::<8912>::default();
        serializer.serialize_value(self).unwrap();
        let bytes = IVec::from(serializer.into_serializer().into_inner().as_ref());

        unsafe { OwnedArchivedChange::<T>::new(bytes) }
    }
}

/// Creates an [`EncodedChanges`].
///
/// Prevents duplicates, keeping the latest change. Also sorts the changes by Morton order for efficient DB insertion.
#[derive(Default)]
pub struct ChangeEncoder {
    added_changes: SmallKeyHashMap<ChunkDbKey, Change<CompressedChunk>>,
}

impl ChangeEncoder {
    pub fn add_compressed_change(&mut self, key: ChunkDbKey, change: Change<CompressedChunk>) {
        self.added_changes.insert(key, change);
    }

    /// Sorts the changes by Morton key and converts them to `IVec` key-value pairs for `sled`.
    pub fn encode(self) -> EncodedChanges<CompressedChunk> {
        // Serialize values.
        let mut changes: Vec<_> = self
            .added_changes
            .into_iter()
            .map(|(key, change)| (key, change.serialize_to_ivec()))
            .collect();

        // Sort by the ord key.
        changes.sort_by_key(|(key, _change)| *key);

        // Serialize the keys.
        let changes: Vec<_> = changes
            .into_iter()
            .map(|(key, change)| (IVec::from(key.to_be_bytes().as_ref()), change))
            .collect();

        EncodedChanges { changes }
    }
}

/// A set of [Change]s to be atomically applied to a [`MapDb`](crate::MapDb).
///
/// Should be created with a [`ChangeEncoder`], which is guaranteed to drop duplicate changes on the same key, keeping only the
/// latest changes.
#[derive(Clone, Default)]
pub struct EncodedChanges<T> {
    pub changes: Vec<(IVec, OwnedArchivedChange<T>)>,
}

/// We use this format for all changes stored in the working tree and backup tree.
///
/// Any values written to the working tree must be [`Change::Insert`] variants, but [`Change::Remove`]s are allowed and
/// necessary inside the backup tree.
///
/// By using the same format for values in both trees, we don't need to re-serialize them when moving any entry from the working
/// tree to the backup tree.
#[derive(Clone)]
pub struct OwnedArchivedChange<T> {
    bytes: IVec,
    marker: PhantomData<T>,
}

impl<T> OwnedArchivedChange<T> {
    /// # Safety
    ///
    /// `bytes` must be a valid [`Archive`] representation for `T`.
    pub unsafe fn new(bytes: IVec) -> Self {
        Self {
            bytes,
            marker: PhantomData,
        }
    }

    pub fn take_bytes(self) -> IVec {
        self.bytes
    }

    pub fn remove() -> Self
    where
        T: Serialize<CoreSerializer<16, 0>>,
    {
        let mut serializer = CoreSerializer::<16, 0>::default();
        serializer.serialize_value(&Change::<T>::Remove).unwrap();
        let bytes = serializer.into_serializer().into_inner();
        unsafe { OwnedArchivedChange::new(IVec::from(bytes.as_ref())) }
    }
}

impl OwnedArchivedChange<CompressedChunk> {
    pub fn unarchive(&self) -> Change<CompressedChunk> {
        match self.as_ref() {
            ArchivedChange::Insert(value) => Change::Insert(CompressedChunk {
                bytes: Box::from(value.bytes.as_ref()),
            }),
            ArchivedChange::Remove => Change::Remove,
        }
    }
}

impl<T> AsRef<ArchivedChange<T>> for OwnedArchivedChange<T>
where
    T: Archive,
{
    fn as_ref(&self) -> &ArchivedChange<T> {
        unsafe { archived_root::<Change<T>>(self.bytes.as_ref()) }
    }
}
