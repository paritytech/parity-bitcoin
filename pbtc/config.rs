use std::net;
use clap;
use network::Magic;
use {USER_AGENT, REGTEST_USER_AGENT};

pub struct Config {
	pub magic: Magic,
	pub port: u16,
	pub connect: Option<net::SocketAddr>,
	pub seednode: Option<String>,
	pub print_to_console: bool,
	pub inbound_connections: u32,
	pub outbound_connections: u32,
	pub p2p_threads: usize,
	pub db_cache: usize,
	pub data_dir: Option<String>,
	pub user_agent: String,
}

pub const DEFAULT_DB_CACHE: usize = 512;

pub fn parse(matches: &clap::ArgMatches) -> Result<Config, String> {
	let print_to_console = matches.is_present("printtoconsole");
	let magic = match (matches.is_present("testnet"), matches.is_present("regtest")) {
		(true, false) => Magic::Testnet,
		(false, true) => Magic::Regtest,
		(false, false) => Magic::Mainnet,
		(true, true) => return Err("Only one testnet option can be used".into()),
	};

	let (in_connections, out_connections) = match magic {
		Magic::Testnet | Magic::Mainnet | Magic::Other(_) => (10, 10),
		Magic::Regtest => (1, 0),
	};

	let p2p_threads = match magic {
		Magic::Testnet | Magic::Mainnet | Magic::Other(_) => 4,
		Magic::Regtest => 1,
	};

	// to skip idiotic 30 seconds delay in test-scripts
	let user_agent = match magic {
		Magic::Testnet | Magic::Mainnet | Magic::Other(_) => USER_AGENT,
		Magic::Regtest => REGTEST_USER_AGENT,
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

	let db_cache = match matches.value_of("db-cache") {
		Some(s) => try!(s.parse().map_err(|_| "Invalid cache size - should be number in MB".to_owned())),
		None => DEFAULT_DB_CACHE,
	};

	let data_dir = match matches.value_of("data-dir") {
		Some(s) => Some(try!(s.parse().map_err(|_| "Invalid data-dir".to_owned()))),
		None => None,
	};

	let config = Config {
		print_to_console: print_to_console,
		magic: magic,
		port: port,
		connect: connect,
		seednode: seednode,
		inbound_connections: in_connections,
		outbound_connections: out_connections,
		p2p_threads: p2p_threads,
		db_cache: db_cache,
		data_dir: data_dir,
		user_agent: user_agent.to_string(),
	};

	Ok(config)
}
