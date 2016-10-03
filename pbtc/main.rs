//! Parity bitcoin client.

#[macro_use]
extern crate clap;
extern crate tokio_core;
extern crate futures;

extern crate bitcrypto as crypto;
extern crate chain;
extern crate keys;
extern crate primitives;
extern crate script;
extern crate message;
extern crate p2p;

mod config;

use futures::stream::Stream;
use std::net::SocketAddr;
use p2p::connect::{Config as P2PConfig, self};

pub fn event_loop() -> tokio_core::reactor::Core {
	tokio_core::reactor::Core::new().unwrap()
}

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
	let handle = el.handle();

	let p2p_cfg = P2PConfig {
		magic: cfg.magic,
		local_address: SocketAddr::new("127.0.0.1".parse().unwrap(), cfg.port),
		services: Default::default(),
		user_agent: "pbtc".into(),
		start_height: 0,
		relay: false,
	};

	if let Some(connect) = cfg.connect {
		let c = connect::connect(&SocketAddr::new(connect, cfg.magic.port()), &handle, &p2p_cfg);
		let connection = try!(el.run(c).map_err(|_| format!("Connect to {} failed", connect)));
	}

	let listen = try!(connect::listen(&handle, p2p_cfg).map_err(|_| "Cannot start listening".to_owned()));
	let server = listen.for_each(|connection| {
		println!("new connection: {:?}", connection.handshake_result);
		Ok(())
	});

	el.run(server).unwrap();
	Ok(())
}

