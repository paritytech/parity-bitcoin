//! Parity bitcoin client.

#[macro_use]
extern crate clap;
extern crate env_logger;

extern crate keys;
extern crate script;
extern crate message;
extern crate p2p;

mod config;

use std::net::SocketAddr;
use p2p::{P2P, event_loop, forever, NetConfig};

fn main() {
	env_logger::init().unwrap();
	match run() {
		Err(err) => println!("{}", err),
		Ok(_) => (),
	}
}

fn run() -> Result<(), String> {
	let yaml = load_yaml!("cli.yml");
	let matches = clap::App::from_yaml(yaml).get_matches();
	let cfg = try!(config::parse(&matches));

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

	let p2p = P2P::new(p2p_cfg, el.handle());
	try!(p2p.run().map_err(|_| "Failed to start p2p module"));
	el.run(forever()).unwrap();
	Ok(())
}

