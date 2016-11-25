use std::net::SocketAddr;
use sync::create_sync_connection_factory;
use message::Services;
use util::{open_db, init_db, node_table_path};
use {config, p2p, PROTOCOL_VERSION, PROTOCOL_MINIMUM};

pub fn start(cfg: config::Config) -> Result<(), String> {
	let mut el = p2p::event_loop();

	let db = open_db(&cfg);
	try!(init_db(&cfg, &db));

	let nodes_path = node_table_path(&cfg);

	let p2p_cfg = p2p::Config {
		threads: cfg.p2p_threads,
		inbound_connections: cfg.inbound_connections,
		outbound_connections: cfg.outbound_connections,
		connection: p2p::NetConfig {
			protocol_version: PROTOCOL_VERSION,
			protocol_minimum: PROTOCOL_MINIMUM,
			magic: cfg.magic,
			local_address: SocketAddr::new("127.0.0.1".parse().unwrap(), cfg.port),
			services: Services::default().with_network(true),
			user_agent: cfg.user_agent,
			start_height: 0,
			relay: false,
		},
		peers: cfg.connect.map_or_else(|| vec![], |x| vec![x]),
		seeds: cfg.seednode.map_or_else(|| vec![], |x| vec![x]),
		node_table_path: nodes_path,
	};

	let sync_handle = el.handle();
	let sync_connection_factory = create_sync_connection_factory(&sync_handle, cfg.magic, db);

	let p2p = try!(p2p::P2P::new(p2p_cfg, sync_connection_factory, el.handle()).map_err(|x| x.to_string()));
	try!(p2p.run().map_err(|_| "Failed to start p2p module"));
	el.run(p2p::forever()).unwrap();
	Ok(())
}
