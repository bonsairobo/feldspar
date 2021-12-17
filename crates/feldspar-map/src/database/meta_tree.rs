use super::Version;

use rkyv::{
    archived_value,
    ser::{serializers::CoreSerializer, Serializer},
    Archive, Deserialize, Infallible, Serialize,
};
use sled::{
    transaction::{
        abort, ConflictableTransactionResult, TransactionError, TransactionalTree,
        UnabortableTransactionError,
    },
    IVec, Tree,
};

const META_KEY: &'static str = "META";

#[derive(Archive, Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[archive_attr(derive(Eq, PartialEq))]
pub struct MapDbMetadata {
    pub parent_version: Version,
    pub working_version: Version,
}

pub fn open_meta_tree(
    map_name: &str,
    db: &sled::Db,
) -> Result<(Tree, MapDbMetadata), TransactionError> {
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

    Ok((tree, cached_meta))
}

pub fn write_meta(
    txn: &TransactionalTree,
    meta: &MapDbMetadata,
) -> Result<(), UnabortableTransactionError> {
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
) -> Result<Option<OwnedArchivedMapDbMetadata>, UnabortableTransactionError> {
    let data = txn.get(META_KEY)?;
    Ok(data.map(OwnedArchivedMapDbMetadata::new))
}

pub fn read_meta_or_abort(
    txn: &TransactionalTree,
) -> ConflictableTransactionResult<OwnedArchivedMapDbMetadata> {
    if let Some(meta) = read_meta(txn)? {
        Ok(meta)
    } else {
        abort(())
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
        let (tree, cached_meta) = open_meta_tree("mymap", &db).unwrap();

        assert_eq!(cached_meta, MapDbMetadata::default());

        let new_meta = MapDbMetadata {
            parent_version: Version::new(20),
            working_version: Version::new(18),
        };
        let _: Result<(), TransactionError<()>> = tree.transaction(|txn| {
            let _ = write_meta(txn, &new_meta)?;
            Ok(())
        });

        // Re-open to make sure we can refresh the cached value.
        let (tree, cached_meta) = open_meta_tree("mymap", &db).unwrap();
        assert_eq!(cached_meta, new_meta);
    }
}
