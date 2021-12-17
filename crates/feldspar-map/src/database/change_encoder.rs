use super::ChunkDbKey;
use crate::{Change, CompressedChunk, SmallKeyHashMap};

use sled::IVec;

/// Creates a [`EncodedChanges`]. This handles sorting the changes in Morton order and compressing the chunk data.
#[derive(Default)]
pub struct ChangeEncoder {
    // Prevent duplicates, keeping the latest change.
    raw_changes: SmallKeyHashMap<ChunkDbKey, Change<IVec>>,
}

impl ChangeEncoder {
    pub fn add_compressed_change(&mut self, key: ChunkDbKey, change: Change<CompressedChunk>) {
        self.raw_changes
            .insert(key, change.map(|v| IVec::from(v.bytes)));
    }

    /// Sorts the changes by Morton key and converts them to `IVec` key-value pairs for `sled`.
    pub fn encode(self) -> EncodedChanges {
        // Sort them by the Ord key.
        let mut changes: Vec<_> = self.raw_changes.into_iter().collect();
        changes.sort_by_key(|(key, _change)| *key);

        let changes: Vec<_> = changes
            .into_iter()
            .map(|(key, change)| (IVec::from(key.to_be_bytes().as_ref()), change))
            .collect();

        EncodedChanges { changes }
    }
}

/// A set of [Change]s to be atomically applied to a [`MapDb`](crate::MapDb).
///
/// This is guaranteed to drop duplicate changes on the same key, keeping only the latest changes.
///
/// Can be created with a [`ChangeEncoder`].
#[derive(Clone, Default)]
pub struct EncodedChanges {
    pub changes: Vec<(IVec, Change<IVec>)>,
}
