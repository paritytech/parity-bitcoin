use std::net;
use clap;
use message::Magic;

pub struct Config {
	pub magic: Magic,
	pub port: u16,
	pub connect: Option<net::SocketAddr>,
	pub seednode: Option<String>,
	pub print_to_console: bool,
	pub use_disk_database: bool,
}

pub fn parse(matches: &clap::ArgMatches) -> Result<Config, String> {
	let print_to_console = matches.is_present("printtoconsole");
	let use_disk_database = matches.is_present("diskdb");
	let magic = match matches.is_present("testnet") {
		true => Magic::Testnet,
		false => Magic::Mainnet,
	};

	let port = match matches.value_of("port") {
		Some(port) => try!(port.parse().map_err(|_| "Invalid port".to_owned())),
		None => magic.port(),
	};

	let connect = match matches.value_of("connect") {
		Some(s) => Some(try!(match s.parse::<net::SocketAddr>() {
			Err(_) => s.parse::<net::IpAddr>()
				.map(|ip| net::SocketAddr::new(ip, magic.port()))
				.map_err(|_| "Invalid connect".to_owned()),
			Ok(a) => Ok(a),
		})),
		None => None,
	};

	let seednode = match matches.value_of("seednode") {
		Some(s) => Some(try!(s.parse().map_err(|_| "Invalid seednode".to_owned()))),
		None => None,
	};

	let config = Config {
		print_to_console: print_to_console,
		magic: magic,
		port: port,
		connect: connect,
		seednode: seednode,
		use_disk_database: use_disk_database,
	};

	Ok(config)
}
