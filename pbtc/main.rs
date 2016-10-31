//! Parity bitcoin client.

#[macro_use]
extern crate clap;
extern crate env_logger;

extern crate db;
extern crate chain;
extern crate keys;
extern crate script;
extern crate message;
extern crate p2p;
extern crate sync;
extern crate import;

mod config;

use std::env;
use std::sync::Arc;
use std::net::SocketAddr;
use p2p::{P2P, event_loop, forever, NetConfig};
use sync::local_node::LocalNode;
use sync::inbound_connection_factory::InboundConnectionFactory;
use chain::Block;

fn main() {
	env_logger::init().unwrap();
	match run() {
		Err(err) => println!("{}", err),
		Ok(_) => (),
	}
}

fn open_db(use_disk_database: bool) -> Arc<db::Store> {
	match use_disk_database {
		true => {
			let mut db_path = env::home_dir().expect("Failed to get home dir");
			db_path.push(".pbtc");
			db_path.push("db");
			Arc::new(db::Storage::new(db_path).expect("Failed to open database"))
		},
		false => {
			Arc::new(db::TestStorage::default())
		}
	}
}

fn init_db(db: &Arc<db::Store>) {
	// insert genesis block if db is empty
	if db.best_block_number().is_none() {
		// TODO: move to config
		let genesis_block: Block = "0100000000000000000000000000000000000000000000000000000000000000000000003ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a29ab5f49ffff001d1dac2b7c0101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff4d04ffff001d0104455468652054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c6f757420666f722062616e6b73ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000".into();
		db.insert_block(&genesis_block)
			.expect("Failed to insert genesis block to the database");
	}
}

fn import_blockchain(db_path: &str) {
	for (_i, _blk) in import::open_blk_dir(db_path).expect("TODO").enumerate() {
		// import logic goes here
	}
}

fn run() -> Result<(), String> {
	let yaml = load_yaml!("cli.yml");
	let matches = clap::App::from_yaml(yaml).get_matches();
	let cfg = try!(config::parse(&matches));

	if let Some(ref import_path) = cfg.import_path {
		import_blockchain(import_path);
		return Ok(())
	}

	let mut el = event_loop();

	let p2p_cfg = p2p::Config {
		threads: 4,
		protocol_minimum: 70001,
		protocol_maximum: 70017,
		inbound_connections: 10,
		outbound_connections: 10,
		connection: NetConfig {
			magic: cfg.magic,
			local_address: SocketAddr::new("127.0.0.1".parse().unwrap(), cfg.port),
			services: Default::default(),
			user_agent: "pbtc".into(),
			start_height: 0,
			relay: false,
		},
		peers: cfg.connect.map_or_else(|| vec![], |x| vec![x]),
		seeds: cfg.seednode.map_or_else(|| vec![], |x| vec![x]),
	};

	let db = open_db(cfg.use_disk_database);
	init_db(&db);

	let local_sync_node = LocalNode::new(db);
	let local_sync_factory = InboundConnectionFactory::with_local_node(local_sync_node.clone());

	let p2p = P2P::new(p2p_cfg, local_sync_factory, el.handle());
	try!(p2p.run().map_err(|_| "Failed to start p2p module"));
	el.run(forever()).unwrap();
	Ok(())
}

