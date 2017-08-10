use std::net;
use clap;
use network::Magic;
use p2p::InternetProtocol;
use seednodes::{mainnet_seednodes, testnet_seednodes};
use rpc_apis::ApiSet;
use {USER_AGENT, REGTEST_USER_AGENT};
use primitives::hash::H256;
use rpc::HttpConfiguration as RpcHttpConfig;
use verification::VerificationLevel;
use sync::VerificationParameters;

pub struct Config {
	pub magic: Magic,
	pub port: u16,
	pub connect: Option<net::SocketAddr>,
	pub seednodes: Vec<String>,
	pub print_to_console: bool,
	pub inbound_connections: u32,
	pub outbound_connections: u32,
	pub p2p_threads: usize,
	pub db_cache: usize,
	pub data_dir: Option<String>,
	pub user_agent: String,
	pub internet_protocol: InternetProtocol,
	pub rpc_config: RpcHttpConfig,
	pub block_notify_command: Option<String>,
	pub verification_params: VerificationParameters,
}

pub const DEFAULT_DB_CACHE: usize = 512;

pub fn parse(matches: &clap::ArgMatches) -> Result<Config, String> {
	let print_to_console = matches.is_present("print-to-console");
	let magic = match (matches.is_present("testnet"), matches.is_present("regtest")) {
		(true, false) => Magic::Testnet,
		(false, true) => Magic::Regtest,
		(false, false) => Magic::Mainnet,
		(true, true) => return Err("Only one testnet option can be used".into()),
	};

	let (in_connections, out_connections) = match magic {
		Magic::Testnet | Magic::Mainnet | Magic::Other(_) => (10, 10),
		Magic::Regtest | Magic::Unitest => (1, 0),
	};

	let p2p_threads = match magic {
		Magic::Testnet | Magic::Mainnet | Magic::Other(_) => 4,
		Magic::Regtest | Magic::Unitest => 1,
	};

	// to skip idiotic 30 seconds delay in test-scripts
	let user_agent = match magic {
		Magic::Testnet | Magic::Mainnet | Magic::Unitest | Magic::Other(_) => USER_AGENT,
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

	let seednodes = match matches.value_of("seednode") {
		Some(s) => vec![try!(s.parse().map_err(|_| "Invalid seednode".to_owned()))],
		None => match magic {
			Magic::Mainnet => mainnet_seednodes().into_iter().map(Into::into).collect(),
			Magic::Testnet => testnet_seednodes().into_iter().map(Into::into).collect(),
			Magic::Other(_) | Magic::Regtest | Magic::Unitest => Vec::new(),
		},
	};

	let db_cache = match matches.value_of("db-cache") {
		Some(s) => try!(s.parse().map_err(|_| "Invalid cache size - should be number in MB".to_owned())),
		None => DEFAULT_DB_CACHE,
	};

	let data_dir = match matches.value_of("data-dir") {
		Some(s) => Some(try!(s.parse().map_err(|_| "Invalid data-dir".to_owned()))),
		None => None,
	};

	let only_net = match matches.value_of("only-net") {
		Some(s) => try!(s.parse()),
		None => InternetProtocol::default(),
	};

	let rpc_config = try!(parse_rpc_config(magic, matches));

	let block_notify_command = match matches.value_of("blocknotify") {
		Some(s) => Some(try!(s.parse().map_err(|_| "Invalid blocknotify commmand".to_owned()))),
		None => None,
	};

	let verification_level = match matches.value_of("verification-level") {
		Some(s) if s == "full" => VerificationLevel::Full,
		Some(s) if s == "header" => VerificationLevel::Header,
		Some(s) if s == "none" => VerificationLevel::NoVerification,
		Some(s) => return Err(format!("Invalid verification level: {}", s)),
		None => VerificationLevel::Full,
	};

	let verification_edge = match matches.value_of("verification-edge") {
		Some(s) if verification_level != VerificationLevel::Full => {
			let edge: H256 = s.parse().map_err(|_| "Invalid verification edge".to_owned())?;
			edge.reversed()
		},
		_ => magic.default_verification_edge(),
	};

	let config = Config {
		print_to_console: print_to_console,
		magic: magic,
		port: port,
		connect: connect,
		seednodes: seednodes,
		inbound_connections: in_connections,
		outbound_connections: out_connections,
		p2p_threads: p2p_threads,
		db_cache: db_cache,
		data_dir: data_dir,
		user_agent: user_agent.to_string(),
		internet_protocol: only_net,
		rpc_config: rpc_config,
		block_notify_command: block_notify_command,
		verification_params: VerificationParameters {
			verification_level: verification_level,
			verification_edge: verification_edge,
		},
	};

	Ok(config)
}

fn parse_rpc_config(magic: Magic, matches: &clap::ArgMatches) -> Result<RpcHttpConfig, String> {
	let mut config = RpcHttpConfig::with_port(magic.rpc_port());
	config.enabled = !matches.is_present("no-jsonrpc");
	if !config.enabled {
		return Ok(config);
	}

	if let Some(apis) = matches.value_of("jsonrpc-apis") {
		config.apis = ApiSet::List(vec![try!(apis.parse().map_err(|_| "Invalid APIs".to_owned()))].into_iter().collect());
	}
	if let Some(port) = matches.value_of("jsonrpc-port") {
		config.port = try!(port.parse().map_err(|_| "Invalid JSON RPC port".to_owned()));
	}
	if let Some(interface) = matches.value_of("jsonrpc-interface") {
		config.interface = interface.to_owned();
	}
	if let Some(cors) = matches.value_of("jsonrpc-cors") {
		config.cors = Some(vec![try!(cors.parse().map_err(|_| "Invalid JSON RPC CORS".to_owned()))]);
	}
	if let Some(hosts) = matches.value_of("jsonrpc-hosts") {
		config.hosts = Some(vec![try!(hosts.parse().map_err(|_| "Invalid JSON RPC hosts".to_owned()))]);
	}

	Ok(config)
}
