use super::{Version, FIRST_VERSION};

use rkyv::{
    archived_value,
    ser::{serializers::CoreSerializer, Serializer},
    Archive, Deserialize, Infallible, Serialize,
};
use sled::{
    transaction::{ConflictableTransactionError, TransactionError, TransactionalTree},
    IVec, Tree,
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
    pub(super) tree: Tree,
    // Zero-copy isn't super important for this tiny struct, so we just copy it for convenience.
    pub(super) cached_meta: MapDbMetadata,
}

impl MetaTree {
    pub fn open(map_name: &str, db: &sled::Db) -> Result<Self, TransactionError> {
        let tree = db.open_tree(format!("{}-meta", map_name))?;

        let cached_meta = tree.transaction(|txn| {
            if let Some(cached_meta) = read_meta(txn)? {
                Ok(cached_meta.unarchive())
            } else {
                // First time opening this tree. Write the initial values.
                let default_meta = MapDbMetadata::default();
                write_meta(txn, &default_meta)?;
                Ok(default_meta)
            }
        })?;

        Ok(Self { tree, cached_meta })
    }

    pub fn write(&mut self, new_meta: &MapDbMetadata) -> Result<(), TransactionError> {
        self.tree.transaction(|txn| write_meta(txn, new_meta))?;
        self.cached_meta = *new_meta;
        Ok(())
    }
}

pub fn write_meta(
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

pub struct OwnedArchivedMapDbMetadata {
    bytes: IVec,
}

impl OwnedArchivedMapDbMetadata {
    pub fn new(bytes: IVec) -> Self {
        Self { bytes }
    }

    pub fn unarchive(&self) -> MapDbMetadata {
        self.as_ref().deserialize(&mut Infallible).unwrap()
    }
}

impl AsRef<ArchivedMapDbMetadata> for OwnedArchivedMapDbMetadata {
    fn as_ref(&self) -> &ArchivedMapDbMetadata {
        unsafe { archived_value::<MapDbMetadata>(self.bytes.as_ref(), 0) }
    }
}

pub fn read_meta(
    txn: &TransactionalTree,
) -> Result<Option<OwnedArchivedMapDbMetadata>, ConflictableTransactionError> {
    let data = txn.get(META_KEY)?;
    Ok(data.map(OwnedArchivedMapDbMetadata::new))
}

pub fn update_current_version(
    txn: &TransactionalTree,
    new_version: Version,
) -> Result<(), ConflictableTransactionError> {
    // TODO: one liner?
    // https://github.com/rkyv/rkyv/issues/232
    let mut serializer = CoreSerializer::<32, 0>::default();
    serializer.serialize_value(&new_version).unwrap();
    let bytes = serializer.into_serializer().into_inner();

    let new_meta = if let Some(current) = read_meta(txn)? {
        MapDbMetadata {
            current_version: new_version,
            bulk_version: Version::new(current.as_ref().current_version.number),
        }
    } else {
        // First time opening this tree.
        MapDbMetadata {
            current_version: new_version,
            bulk_version: FIRST_VERSION,
        }
    };
    write_meta(txn, &new_meta)
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

        assert_eq!(tree.cached_meta, MapDbMetadata::default());

        let new_meta = MapDbMetadata {
            current_version: Version::new(20),
            bulk_version: Version::new(18),
        };
        tree.write(&new_meta).unwrap();

        assert_eq!(tree.cached_meta, new_meta);

        // Re-open to make sure we can refresh the cached value.
        let tree = MetaTree::open("mymap", &db).unwrap();
        assert_eq!(tree.cached_meta, new_meta);
    }
}
