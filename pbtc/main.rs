//! Parity bitcoin client.

#[macro_use]
extern crate clap;

extern crate bitcrypto as crypto;
extern crate keys;
extern crate script;
extern crate message;
extern crate p2p;

mod config;

use std::net::SocketAddr;
use p2p::{net, event_loop};

fn main() {
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
		connection: net::Config {
			magic: cfg.magic,
			local_address: SocketAddr::new("127.0.0.1".parse().unwrap(), cfg.port),
			services: Default::default(),
			user_agent: "pbtc".into(),
			start_height: 0,
			relay: false,
		},
		seednodes: cfg.seednode.map_or_else(|| vec![], |x| vec![x]),
		limited_connect: cfg.connect.map_or(None, |x| Some(vec![x])),
	};

	let handle = el.handle();
	let server = try!(p2p::run(p2p_cfg, &handle).map_err(|_| "Failed to start p2p module"));
	el.run(server).unwrap();
	Ok(())
}

