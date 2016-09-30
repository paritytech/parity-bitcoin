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
	let _cfg = parse(&matches);
}

struct Config<'a> {
	magic: Magic,
	port: u16,
	connect: Option<&'a str>,
	seednode: Option<&'a str>,
}

fn parse<'a>(matches: &'a clap::ArgMatches<'a>) -> Result<Config<'a>, String> {
	let magic = match matches.is_present("testnet") {
		true => Magic::Testnet,
		false => Magic::Mainnet,
	};

	let port = match matches.value_of("port") {
		Some(port) => try!(port.parse().map_err(|_| "Invalid port".to_owned())),
		None => match magic {
			Magic::Mainnet => 8333,
			Magic::Testnet => 18333,
		},
	};

	let config = Config {
		magic: magic,
		port: port,
		connect: matches.value_of("connect"),
		seednode: matches.value_of("seednode"),
	};

	Ok(config)
}
