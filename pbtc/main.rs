//! Parity bitcoin client.

#[macro_use]
extern crate clap;

extern crate bitcrypto as crypto;
extern crate chain;
extern crate keys;
extern crate primitives;
extern crate script;

fn main() {
	let yaml = load_yaml!("cli.yml");
	let matches = clap::App::from_yaml(yaml).get_matches();

	if let Some(ip) = matches.value_of("connect") {
		println!("Connecting to ip: {}", ip);
	}
}
