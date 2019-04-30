use std::sync::Arc;
use std::path::PathBuf;
use std::fs::create_dir_all;
use app_dirs::{app_dir, AppDataType};
use {storage, APP_INFO};
use db;
use config::Config;

pub fn open_db(data_dir: &Option<String>, db_cache: usize) -> storage::SharedStore {
	let db_path = match *data_dir {
		Some(ref data_dir) => custom_path(&data_dir, "db"),
		None => app_dir(AppDataType::UserData, &APP_INFO, "db").expect("Failed to get app dir"),
	};
	Arc::new(db::BlockChainDatabase::open_at_path(db_path, db_cache).expect("Failed to open database"))
}

pub fn node_table_path(cfg: &Config) -> PathBuf {
	let mut node_table = match cfg.data_dir {
		Some(ref data_dir) => custom_path(&data_dir, "p2p"),
		None => app_dir(AppDataType::UserData, &APP_INFO, "p2p").expect("Failed to get app dir"),
	};
	node_table.push("nodes.csv");
	node_table
}

pub fn init_db(cfg: &Config) -> Result<(), String> {
	// insert genesis block if db is empty
	let genesis_block = cfg.network.genesis_block();
	match cfg.db.block_hash(0) {
		Some(ref db_genesis_block_hash) if db_genesis_block_hash != genesis_block.hash() => Err("Trying to open database with incompatible genesis block".into()),
		Some(_) => Ok(()),
		None => {
			let hash = genesis_block.hash().clone();
			cfg.db.insert(genesis_block).expect("Failed to insert genesis block to the database");
			cfg.db.canonize(&hash).expect("Failed to canonize genesis block");
			Ok(())
		}
	}
}

fn custom_path(data_dir: &str, sub_dir: &str) -> PathBuf {
	let mut path = PathBuf::from(data_dir);
	path.push(sub_dir);
	create_dir_all(&path).expect("Failed to get app dir");
	path
}
