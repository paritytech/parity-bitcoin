use ser::{
	Serializable, Stream,
	Reader, Error as ReaderError
};
use common::Command;
use messages::{Version, Addr};

#[derive(Debug, PartialEq)]
pub enum Payload {
	Version(Version),
	Verack,
	Addr(Addr),
}

impl Payload {
	pub fn command(&self) -> Command {
		match *self {
			Payload::Version(_) => "version".into(),
			Payload::Verack => "verack".into(),
			Payload::Addr(_) => "addr".into(),
		}
	}

	pub fn deserialize_payload(reader: &mut Reader, command: &Command) -> Result<Payload, ReaderError> {
		match &command.to_string() as &str {
			"version" => reader.read().map(Payload::Version),
			"verack" => Ok(Payload::Verack),
			_ => Err(ReaderError::MalformedData),
		}
	}
}

impl Serializable for Payload {
	fn serialize(&self, stream: &mut Stream) {
		match *self {
			Payload::Version(ref version) => {
				stream.append(version);
			},
			Payload::Verack => {},
			Payload::Addr(ref addr) => {
				stream.append(addr);
			},
		}
	}
}
