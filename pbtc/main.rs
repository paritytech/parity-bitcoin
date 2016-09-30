//! Parity bitcoin client.

#[macro_use]
extern crate clap;

extern crate bitcrypto as crypto;
extern crate chain;
extern crate keys;
extern crate primitives;
extern crate script;
extern crate net;
extern crate p2p;

use net::common::Magic;

fn main() {
	let yaml = load_yaml!("cli.yml");
	let matches = clap::App::from_yaml(yaml).get_matches();

	let _magic = match matches.is_present("testnet") {
		true => Magic::Testnet,
		false => Magic::Mainnet,
	};

	if let Some(ip) = matches.value_of("connect") {
		println!("Connecting to ip: {}", ip);
	}

	if let Some(ip) = matches.value_of("seednode") {
		println!("Seednode ip: {}", ip);
	}
}
