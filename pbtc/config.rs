use clap;
use net::common::Magic;

pub struct Config<'a> {
	pub magic: Magic,
	pub port: u16,
	pub connect: Option<&'a str>,
	pub seednode: Option<&'a str>,
}

pub fn parse<'a>(matches: &'a clap::ArgMatches<'a>) -> Result<Config<'a>, String> {
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
