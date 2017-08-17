extern crate ansi_term;
extern crate log;
extern crate env_logger;
extern crate time;

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
		let log_level;
		match record.level() {
			LogLevel::Error => log_level = Color::Fixed(9).bold().paint(record.level().to_string()),
			LogLevel::Warn  => log_level = Color::Fixed(11).bold().paint(record.level().to_string()),
			LogLevel::Info  => log_level = Color::Fixed(10).paint(record.level().to_string()),
			LogLevel::Debug => log_level = Color::Fixed(14).paint(record.level().to_string()),
			LogLevel::Trace => log_level = Color::Fixed(12).paint(record.level().to_string()),
		}
		format!("{} {} {} {}", Color::Black.bold().paint(timestamp), log_level, record.target(), record.args())
	}
}

pub fn init<T>(filters: &str, formatter: T) where T: LogFormatter {
	let mut builder = LogBuilder::new();
	builder.parse(filters);
	builder.format(move |record| formatter.format(record));
	builder.init().expect("Logger can be initialized only once");
}
