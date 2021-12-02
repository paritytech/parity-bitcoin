//! Parity bitcoin client.

extern crate app_dirs;
extern crate chain;
#[macro_use]
extern crate clap;
extern crate db;
extern crate env_logger;
extern crate import;
extern crate keys;
extern crate libc;
#[macro_use]
extern crate log;
extern crate logs;
extern crate message;
extern crate network;
extern crate p2p;
extern crate primitives;
extern crate rpc as ethcore_rpc;
extern crate script;
extern crate storage;
extern crate sync;
extern crate verification;

use app_dirs::AppInfo;

mod commands;
mod config;
mod rpc;
mod rpc_apis;
mod seednodes;
mod util;

pub const APP_INFO: AppInfo = AppInfo {
	name: "pbtc",
	author: "Parity",
};
pub const PROTOCOL_VERSION: u32 = 70_014;
pub const PROTOCOL_MINIMUM: u32 = 70_001;
pub const USER_AGENT: &'static str = "pbtc";
pub const REGTEST_USER_AGENT: &'static str = "/Satoshi:0.12.1/";
pub const LOG_INFO: &'static str = "sync=info";

fn main() {
	// Always print backtrace on panic.
	::std::env::set_var("RUST_BACKTRACE", "1");

	if let Err(err) = run() {
		println!("{}", err);
	}
}

fn run() -> Result<(), String> {
	let yaml = load_yaml!("cli.yml");
	let matches = clap::App::from_yaml(yaml).get_matches();
	let cfg = config::parse(&matches)?;

	if !cfg.quiet {
		if cfg!(windows) {
			logs::init(LOG_INFO, logs::DateLogFormatter);
		} else {
			logs::init(LOG_INFO, logs::DateAndColorLogFormatter);
		}
	} else {
		env_logger::init();
	}

	match matches.subcommand() {
		("import", Some(import_matches)) => commands::import(cfg, import_matches),
		("rollback", Some(rollback_matches)) => commands::rollback(cfg, rollback_matches),
		_ => commands::start(cfg),
	}
}
