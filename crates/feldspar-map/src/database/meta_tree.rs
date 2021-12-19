use super::{AbortReason, ArchivedIVec, Version};

use rkyv::{
    ser::{serializers::CoreSerializer, Serializer},
    Archive, Deserialize, Serialize,
};
use sled::{
    transaction::{TransactionError, TransactionalTree, UnabortableTransactionError},
    Tree,
};

const META_KEY: &'static str = "META";

#[derive(Archive, Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[archive_attr(derive(Eq, PartialEq))]
pub struct MapDbMetadata {
    pub grandparent_version: Option<Version>,
    pub parent_version: Option<Version>,
    pub working_version: Version,
}

pub fn open_meta_tree(
    map_name: &str,
    db: &sled::Db,
) -> Result<(Tree, MapDbMetadata), TransactionError<AbortReason>> {
    let tree = db.open_tree(format!("{}-meta", map_name))?;

    let cached_meta = tree.transaction(|txn| {
        if let Some(cached_meta) = read_meta(txn)? {
            Ok(cached_meta.deserialize())
        } else {
            // First time opening this tree. Write the initial values.
            let working_version = Version::new(txn.generate_id()?);
            let meta = MapDbMetadata {
                grandparent_version: None,
                parent_version: None,
                working_version,
            };
            write_meta(txn, &meta)?;
            Ok(meta)
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
    let mut serializer = CoreSerializer::<40, 0>::default();
    serializer.serialize_value(meta).unwrap();
    let bytes = serializer.into_serializer().into_inner();

    txn.insert(META_KEY, bytes.as_ref())?;

    Ok(())
}

pub fn read_meta(
    txn: &TransactionalTree,
) -> Result<Option<ArchivedIVec<MapDbMetadata>>, UnabortableTransactionError> {
    let data = txn.get(META_KEY)?;
    Ok(data.map(|b| unsafe { ArchivedIVec::<MapDbMetadata>::new(b) }))
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
            grandparent_version: None,
            parent_version: Some(Version::new(20)),
            working_version: Version::new(18),
        };
        let _: Result<(), TransactionError<()>> = tree.transaction(|txn| {
            write_meta(txn, &new_meta)?;
            Ok(())
        });

        // Re-open to make sure we can refresh the cached value.
        let (_tree, cached_meta) = open_meta_tree("mymap", &db).unwrap();
        assert_eq!(cached_meta, new_meta);
    }
}
