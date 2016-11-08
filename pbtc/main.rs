//! Parity bitcoin client.

#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]

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
