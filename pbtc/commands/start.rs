use std::net::SocketAddr;
use sync::create_sync_connection_factory;
use message::Services;
use util::{open_db, init_db, node_table_path};
use {config, p2p};

pub fn start(cfg: config::Config) -> Result<(), String> {
	let mut el = p2p::event_loop();

	let db = open_db(&cfg);
	try!(init_db(&cfg, &db));

	let p2p_cfg = p2p::Config {
		threads: 4,
		protocol_minimum: 70001,
		protocol_maximum: 70017,
		inbound_connections: 10,
		outbound_connections: 10,
		connection: p2p::NetConfig {
			magic: cfg.magic,
			local_address: SocketAddr::new("127.0.0.1".parse().unwrap(), cfg.port),
			services: Services::default().with_network(true),
			user_agent: "pbtc".into(),
			start_height: 0,
			relay: false,
		},
		peers: cfg.connect.map_or_else(|| vec![], |x| vec![x]),
		seeds: cfg.seednode.map_or_else(|| vec![], |x| vec![x]),
		node_table_path: node_table_path(),
	};

	let sync_handle = el.handle();
	let sync_connection_factory = create_sync_connection_factory(&sync_handle, db);

	let p2p = try!(p2p::P2P::new(p2p_cfg, sync_connection_factory, el.handle()).map_err(|x| x.to_string()));
	try!(p2p.run().map_err(|_| "Failed to start p2p module"));
	el.run(p2p::forever()).unwrap();
	Ok(())
}
