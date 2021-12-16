use super::Version;

use rkyv::{
    archived_value,
    ser::{serializers::CoreSerializer, Serializer},
    Archive, Deserialize, Infallible, Serialize,
};
use sled::{
    transaction::{ConflictableTransactionError, TransactionError, TransactionalTree},
    Tree,
};

const META_KEY: &'static str = "META";

#[derive(Archive, Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[archive_attr(derive(Eq, PartialEq))]
pub struct MapDbMetadata {
    pub current_version: Version,
    pub bulk_version: Version,
}

/// Mapping from `&str` to structured metadata, like [`MapDbMetadata`].
pub struct MetaTree {
    tree: Tree,
    // Zero-copy isn't super important for this tiny struct, so we just copy it for convenience.
    meta: MapDbMetadata,
}

impl MetaTree {
    pub fn open(map_name: &str, db: &sled::Db) -> Result<Self, TransactionError> {
        let tree = db.open_tree(format!("{}-meta", map_name))?;

        let meta = tree.transaction(|txn| {
            if let Some(data) = txn.get(META_KEY)? {
                let archived = unsafe { archived_value::<MapDbMetadata>(data.as_ref(), 0) };
                Ok(archived.deserialize(&mut Infallible).unwrap())
            } else {
                // First time opening this tree. Write the initial values.
                let default_meta = MapDbMetadata::default();
                Self::_write_meta(txn, &default_meta)?;
                Ok(default_meta)
            }
        })?;

        Ok(Self { tree, meta })
    }

    pub fn cached_meta(&self) -> &MapDbMetadata {
        &self.meta
    }

    pub fn write(&mut self, new_meta: &MapDbMetadata) -> Result<(), TransactionError> {
        self.tree
            .transaction(|txn| Self::_write_meta(txn, new_meta))?;
        self.meta = *new_meta;
        Ok(())
    }

    fn _write_meta(
        txn: &TransactionalTree,
        meta: &MapDbMetadata,
    ) -> Result<(), ConflictableTransactionError> {
        // TODO: one liner?
        // https://github.com/rkyv/rkyv/issues/232
        let mut serializer = CoreSerializer::<32, 0>::default();
        serializer.serialize_value(meta).unwrap();
        let bytes = serializer.into_serializer().into_inner();

        txn.insert(META_KEY, bytes.as_ref())?;

        Ok(())
    }
}

// ████████╗███████╗███████╗████████╗
// ╚══██╔══╝██╔════╝██╔════╝╚══██╔══╝
//    ██║   █████╗  ███████╗   ██║
//    ██║   ██╔══╝  ╚════██║   ██║
//    ██║   ███████╗███████║   ██║
//    ╚═╝   ╚══════╝╚══════╝   ╚═╝

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_write_and_reopen_meta_tree() {
        let db = sled::Config::default().temporary(true).open().unwrap();
        let mut tree = MetaTree::open("mymap", &db).unwrap();

        assert_eq!(tree.cached_meta(), &MapDbMetadata::default());

        let new_meta = MapDbMetadata {
            current_version: Version::new(20),
            bulk_version: Version::new(18),
        };
        tree.write(&new_meta).unwrap();

        assert_eq!(tree.cached_meta(), &new_meta);

        // Re-open to make sure we can refresh the cached value.
        let tree = MetaTree::open("mymap", &db).unwrap();
        assert_eq!(tree.cached_meta(), &new_meta);
    }
}
