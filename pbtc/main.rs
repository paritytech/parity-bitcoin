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

mod config;

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

	if let Some(_connect) = cfg.connect {

	}

	Ok(())
}

