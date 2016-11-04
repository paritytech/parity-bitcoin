use std::sync::Arc;
use std::path::PathBuf;
use app_dirs::{app_dir, AppDataType};
use chain::Block;
use {db, APP_INFO};

pub fn open_db(use_disk_database: bool) -> Arc<db::Store> {
	match use_disk_database {
		true => {
			let db_path = app_dir(AppDataType::UserData, &APP_INFO, "db").expect("Failed to get app dir");
			Arc::new(db::Storage::new(db_path).expect("Failed to open database"))
		},
		false => {
			Arc::new(db::TestStorage::default())
		}
	}
}

pub fn node_table_path() -> PathBuf {
	let mut node_table = app_dir(AppDataType::UserData, &APP_INFO, "p2p").expect("Failed to get app dir");
	node_table.push("nodes.csv");
	node_table
}

pub fn init_db(db: &Arc<db::Store>) {
	// insert genesis block if db is empty
	if db.best_block().is_none() {
		// TODO: move to config
		let genesis_block: Block = "0100000000000000000000000000000000000000000000000000000000000000000000003ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a29ab5f49ffff001d1dac2b7c0101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff4d04ffff001d0104455468652054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c6f757420666f722062616e6b73ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000".into();
		db.insert_block(&genesis_block)
			.expect("Failed to insert genesis block to the database");
	}
}
