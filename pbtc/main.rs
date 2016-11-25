//! Parity bitcoin client.

#[macro_use]
extern crate clap;
#[macro_use]
extern crate log;
extern crate env_logger;
extern crate app_dirs;

extern crate db;
extern crate chain;
extern crate keys;
extern crate script;
extern crate message;
extern crate p2p;
extern crate sync;
extern crate import;

mod commands;
mod config;
mod util;

use app_dirs::AppInfo;

pub const APP_INFO: AppInfo = AppInfo { name: "pbtc", author: "Parity" };
pub const PROTOCOL_VERSION: u32 = 70_014;
pub const PROTOCOL_MINIMUM: u32 = 70_001;
pub const USER_AGENT: &'static str = "pbtc";
pub const REGTEST_USER_AGENT: &'static str = "/Satoshi:0.12.1/";

fn main() {
	env_logger::init().unwrap();
	if let Err(err) = run() {
		println!("{}", err);
	}
}

fn run() -> Result<(), String> {
	let yaml = load_yaml!("cli.yml");
	let matches = clap::App::from_yaml(yaml).get_matches();
	let cfg = try!(config::parse(&matches));

	match matches.subcommand() {
		("import", Some(import_matches)) => commands::import(cfg, import_matches),
		_ => commands::start(cfg),
	}
}
