use super::{ArchivedIVec, ChunkDbKey};
use crate::chunk::CompressedChunk;
use crate::core::rkyv::{
    ser::{serializers::CoreSerializer, Serializer},
    AlignedBytes, AlignedVec, Archive, Archived, Deserialize, Serialize,
};
use crate::core::{NoSharedAllocSerializer, SmallKeyHashMap};

use sled::IVec;

#[derive(Archive, Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[archive(crate = "crate::core::rkyv")]
pub enum Change<T> {
    Insert(T),
    Remove,
}

impl<T> Change<T> {
    pub fn unwrap_insert(self) -> T {
        match self {
            Change::Insert(x) => x,
            Change::Remove => panic!("Unwrapped on Change::Remove"),
        }
    }

    pub fn map<S>(self, mut f: impl FnMut(T) -> S) -> Change<S> {
        match self {
            Change::Insert(x) => Change::Insert(f(x)),
            Change::Remove => Change::Remove,
        }
    }

    pub fn serialize(&self) -> AlignedVec
    where
        T: Serialize<NoSharedAllocSerializer<8912>>,
    {
        let mut serializer = NoSharedAllocSerializer::<8912>::default();
        serializer.serialize_value(self).unwrap();
        serializer.into_serializer().into_inner()
    }

    pub fn serialize_remove<const N: usize>() -> AlignedBytes<N>
    where
        T: Serialize<CoreSerializer<N, 0>>,
    {
        let mut serializer = CoreSerializer::<N, 0>::default();
        serializer.serialize_value(&Change::<T>::Remove).unwrap();
        serializer.into_serializer().into_inner()
    }
}

impl<T> ArchivedChange<T>
where
    T: Archive,
{
    pub fn get_insert_data(&self) -> Option<&Archived<T>> {
        match self {
            Self::Insert(data) => Some(data),
            Self::Remove => None,
        }
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
            .map(|(key, change)| {
                (key, unsafe {
                    // PERF: sad that we can't serialize directly into an IVec
                    ArchivedIVec::new(IVec::from(change.serialize().as_ref()))
                })
            })
            .collect();

        // Sort by the ord key.
        changes.sort_by_key(|(key, _change)| *key);

        // Serialize the keys.
        let changes: Vec<_> = changes
            .into_iter()
            .map(|(key, change)| (IVec::from(key.into_sled_key().as_ref()), change))
            .collect();

        EncodedChanges { changes }
    }
}

/// A set of [Change]s to be atomically applied to a [`MapDb`](crate::MapDb).
///
/// Should be created with a [`ChangeEncoder`], which is guaranteed to drop duplicate changes on the same key, keeping only the
/// latest changes.
#[derive(Clone, Debug, Default)]
pub struct EncodedChanges<T> {
    pub changes: Vec<(IVec, ArchivedChangeIVec<T>)>,
}

/// We use this format for all changes stored in the working tree and backup tree.
///
/// Any values written to the working tree must be [`Change::Insert`] variants, but [`Change::Remove`]s are allowed and
/// necessary inside the backup tree.
///
/// By using the same format for values in both trees, we don't need to re-serialize them when moving any entry from the working
/// tree to the backup tree.
pub type ArchivedChangeIVec<T> = ArchivedIVec<Change<T>>;

// ████████╗███████╗███████╗████████╗
// ╚══██╔══╝██╔════╝██╔════╝╚══██╔══╝
//    ██║   █████╗  ███████╗   ██║
//    ██║   ██╔══╝  ╚════██║   ██║
//    ██║   ███████╗███████║   ██║
//    ╚═╝   ╚══════╝╚══════╝   ╚═╝

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::Chunk;
    use crate::core::archived_buf::ArchivedBuf;

    use sled::IVec;

    #[test]
    fn deserialize_remove_bytes() {
        // This needs to be 12! Leaving empty space at the end of the AlignedBytes will cause archive_root to fail.
        let remove_bytes: ArchivedBuf<Change<CompressedChunk>, AlignedBytes<12>> =
            unsafe { ArchivedBuf::new(Change::<CompressedChunk>::serialize_remove::<12>()) };
        assert_eq!(remove_bytes.deserialize(), Change::Remove);
    }

    #[test]
    fn deserialize_insert_bytes() {
        let original = Change::Insert(Chunk::default().compress());
        let serialized = unsafe {
            ArchivedIVec::<Change<CompressedChunk>>::new(IVec::from(original.serialize().as_ref()))
        };
        let deserialized = serialized.deserialize();
        assert_eq!(deserialized, original);
    }
}
