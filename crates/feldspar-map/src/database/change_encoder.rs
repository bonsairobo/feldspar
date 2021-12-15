use super::ChunkDbKey;
use crate::{Change, CompressedChunk};

use sled::IVec;

/// Creates a [`EncodedChanges`]. This handles sorting the changes in Morton order and compressing the chunk data.
#[derive(Default)]
pub struct ChangeEncoder {
    raw_changes: Vec<(ChunkDbKey, Change<IVec>)>,
}

impl ChangeEncoder {
    pub fn add_compressed_change(&mut self, key: ChunkDbKey, change: Change<CompressedChunk>) {
        self.raw_changes
            .push((key, change.map(|v| IVec::from(v.bytes))));
    }

    /// Sorts the changes by Morton key and converts them to `IVec` key-value pairs for `sled`.
    pub fn encode(mut self) -> EncodedChanges {
        // Sort them by the Ord key.
        self.raw_changes.sort_by_key(|(key, _change)| *key);

        let changes: Vec<_> = self
            .raw_changes
            .into_iter()
            .map(|(key, change)| (IVec::from(key.to_be_bytes().as_ref()), change))
            .collect();

        EncodedChanges { changes }
    }
}

/// A set of [Change]s to be atomically applied to a [`MapDb`](crate::MapDb).
///
/// Can be created with a [`ChangeEncoder`].
#[derive(Default)]
pub struct EncodedChanges {
    pub changes: Vec<(IVec, Change<IVec>)>,
}

impl From<EncodedChanges> for sled::Batch {
    fn from(batch: EncodedChanges) -> Self {
        let mut new_batch = sled::Batch::default();
        for (key_bytes, change) in batch.changes.into_iter() {
            match change {
                Change::Insert(chunk_bytes) => {
                    new_batch.insert(key_bytes.as_ref(), chunk_bytes);
                }
                Change::Remove => new_batch.remove(key_bytes.as_ref()),
            }
        }
        new_batch
    }
}
