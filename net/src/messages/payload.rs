use ser::{
	Serializable, Stream,
	Reader, Error as ReaderError
};
use common::Command;
use messages::{Version, Addr, AddrBelow31402};

pub fn deserialize_payload(data: &[u8], version: u32, command: &Command) -> Result<Payload, ReaderError> {
	let mut reader = Reader::new(data);
	match &command.to_string() as &str {
		"version" => reader.read().map(Payload::Version),
		"verack" => Ok(Payload::Verack),
		"addr" => match version >= 31402 {
			true => reader.read().map(Payload::Addr),
			false => reader.read().map(Payload::AddrBelow31402),
		},
		_ => Err(ReaderError::MalformedData),
	}
}

#[derive(Debug, PartialEq)]
pub enum Payload {
	Version(Version),
	Verack,
	Addr(Addr),
	AddrBelow31402(AddrBelow31402),
}

impl Payload {
	pub fn command(&self) -> Command {
		match *self {
			Payload::Version(_) => "version".into(),
			Payload::Verack => "verack".into(),
			Payload::Addr(_) | Payload::AddrBelow31402(_) => "addr".into(),
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
			Payload::AddrBelow31402(ref addr) => {
				stream.append(addr);
			},
		}
	}
}
