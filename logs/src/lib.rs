extern crate ansi_term;
extern crate log;
extern crate env_logger;
extern crate time;

use ansi_term::Colour as Color;
use log::LogRecord;
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
		format!("{} {} {} {}", Color::Black.bold().paint(timestamp), record.level(), record.target(), record.args())
	}
}

pub fn init<T>(filters: &str, formatter: T) where T: LogFormatter {
	let mut builder = LogBuilder::new();
	builder.parse(filters);
	builder.format(move |record| formatter.format(record));
	builder.init().expect("Logger can be initialized only once");
}
