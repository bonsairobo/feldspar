use sled::{transaction::TransactionError, Tree};

/// A map from [`ChunkDbKey`](crate::ChunkDbKey) to [`ArchivedCompressedChunk`](crate::ArchivedCompressedChunk).
pub struct BulkTree {
    tree: Tree,
}

impl BulkTree {
    pub fn open(db_name: &str, db: &sled::Db) -> Result<Self, TransactionError> {
        let tree = db.open_tree(format!("{}-changes", db_name))?;
        Ok(Self { tree })
    }
}
