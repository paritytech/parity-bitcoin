use std::sync::Arc;
use std::path::PathBuf;
use app_dirs::{app_dir, AppDataType};
use chain::RepresentH256;
use {db, APP_INFO};
use config::Config;

pub fn open_db(_cfg: &Config) -> Arc<db::Store> {
	let db_path = app_dir(AppDataType::UserData, &APP_INFO, "db").expect("Failed to get app dir");
	Arc::new(db::Storage::new(db_path).expect("Failed to open database"))
}

pub fn node_table_path() -> PathBuf {
	let mut node_table = app_dir(AppDataType::UserData, &APP_INFO, "p2p").expect("Failed to get app dir");
	node_table.push("nodes.csv");
	node_table
}

pub fn init_db(cfg: &Config, db: &Arc<db::Store>) -> Result<(), String> {
	// insert genesis block if db is empty
	let genesis_block = cfg.magic.genesis_block();
	match db.block_hash(0) {
		Some(ref db_genesis_block_hash) if db_genesis_block_hash != &genesis_block.hash() => Err("Trying to open database with incompatible genesis block".into()),
		Some(_) => Ok(()),
		None => {
			db.insert_block(&genesis_block).expect("Failed to insert genesis block to the database");
			Ok(())
		}
	}
}
