use sled::Tree;

pub fn open_working_tree(map_name: &str, db: &sled::Db) -> sled::Result<Tree> {
    db.open_tree(format!("{}-working", map_name))
}
