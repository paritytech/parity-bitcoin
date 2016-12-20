use std::net::SocketAddr;
use sync::{create_local_sync_node, create_sync_connection_factory};
use message::Services;
use util::{open_db, init_db, node_table_path};
use {config, p2p, PROTOCOL_VERSION, PROTOCOL_MINIMUM};
use super::super::rpc;

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
			relay: true,
		},
		peers: cfg.connect.map_or_else(|| vec![], |x| vec![x]),
		seeds: cfg.seednodes,
		node_table_path: nodes_path,
		internet_protocol: cfg.internet_protocol,
	};

	let sync_handle = el.handle();
	let local_sync_node = create_local_sync_node(&sync_handle, cfg.magic, db.clone());
	let sync_connection_factory = create_sync_connection_factory(local_sync_node.clone());

	let p2p = try!(p2p::P2P::new(p2p_cfg, sync_connection_factory, el.handle()).map_err(|x| x.to_string()));
	let rpc_deps = rpc::Dependencies {
		network: cfg.magic,
		storage: db,
		local_sync_node: local_sync_node,
		p2p_context: p2p.context().clone(),
		remote: el.remote(),
	};
	let _rpc_server = try!(rpc::new_http(cfg.rpc_config, rpc_deps));

	try!(p2p.run().map_err(|_| "Failed to start p2p module"));
	el.run(p2p::forever()).unwrap();
	Ok(())
}
