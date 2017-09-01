extern crate ansi_term;
extern crate log;
extern crate env_logger;
extern crate time;

use std::env;
use ansi_term::Colour as Color;
use log::{LogRecord, LogLevel};
use env_logger::LogBuilder;

fn strftime() -> String {
	time::strftime("%Y-%m-%d %H:%M:%S %Z", &time::now()).expect("Time is incorrectly formatted")
}

pub trait LogFormatter: Send + Sync + 'static {
	fn format(&self, log_record: &LogRecord) -> String;
}

pub struct DateLogFormatter;

impl LogFormatter for DateLogFormatter {
	fn format(&self, record: &LogRecord) -> String {
		let timestamp = strftime();
		format!("{} {} {} {}", timestamp, record.level(), record.target(), record.args())
	}
}

pub struct DateAndColorLogFormatter;

impl LogFormatter for DateAndColorLogFormatter {
	fn format(&self, record: &LogRecord) -> String {
		let timestamp = strftime();
		let log_level = match record.level() {
			LogLevel::Error => Color::Fixed(9).bold().paint(record.level().to_string()),
			LogLevel::Warn  => Color::Fixed(11).bold().paint(record.level().to_string()),
			LogLevel::Info  => Color::Fixed(10).paint(record.level().to_string()),
			LogLevel::Debug => Color::Fixed(14).paint(record.level().to_string()),
			LogLevel::Trace => Color::Fixed(12).paint(record.level().to_string()),
		};
		format!("{} {} {} {}"
			, Color::Fixed(8).bold().paint(timestamp)
			, log_level
			, Color::Fixed(8).paint(record.target())
			, record.args())
	}
}

pub fn init<T>(filters: &str, formatter: T) where T: LogFormatter {
	let mut builder = LogBuilder::new();
	let filters = match env::var("RUST_LOG") {
		Ok(env_filters) => format!("{},{}", env_filters, filters),
		Err(_) => filters.into(),
	};

	builder.parse(&filters);
	builder.format(move |record| formatter.format(record));
	builder.init().expect("Logger can be initialized only once");
}
