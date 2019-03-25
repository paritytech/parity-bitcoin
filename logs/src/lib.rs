extern crate ansi_term;
extern crate log;
extern crate env_logger;
extern crate time;

use std::env;
use ansi_term::Colour as Color;
use log::{Record, Level};
use env_logger::Builder;
use std::io::Write;

fn strftime() -> String {
	time::strftime("%Y-%m-%d %H:%M:%S %Z", &time::now()).expect("Time is incorrectly formatted")
}

pub trait LogFormatter: Send + Sync + 'static {
	fn format(&self, log_record: &Record) -> String;
}

pub struct DateLogFormatter;

impl LogFormatter for DateLogFormatter {
	fn format(&self, record: &Record) -> String {
		let timestamp = strftime();
		format!("{} {} {} {}", timestamp, record.level(), record.target(), record.args())
	}
}

pub struct DateAndColorLogFormatter;

impl LogFormatter for DateAndColorLogFormatter {
	fn format(&self, record: &Record) -> String {
		let timestamp = strftime();
		let log_level = match record.level() {
			Level::Error => Color::Fixed(9).bold().paint(record.level().to_string()),
			Level::Warn => Color::Fixed(11).bold().paint(record.level().to_string()),
			Level::Info => Color::Fixed(10).paint(record.level().to_string()),
			Level::Debug => Color::Fixed(14).paint(record.level().to_string()),
			Level::Trace => Color::Fixed(12).paint(record.level().to_string()),
		};
		format!("{} {} {} {}"
			, Color::Fixed(8).bold().paint(timestamp)
			, log_level
			, Color::Fixed(8).paint(record.target())
			, record.args())
	}
}

pub fn init<T>(filters: &str, formatter: T) where T: LogFormatter {
	let mut builder = Builder::new();

	let filters = match env::var("RUST_LOG") {
		Ok(env_filters) => format!("{},{}", filters, env_filters),
		Err(_) => filters.into(),
	};

	builder.parse(&filters);
	builder.format(move |buf, record| {
		writeln!(buf, "{}", formatter.format(record))
	});

	builder.init();
}
