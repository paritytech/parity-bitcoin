use ser::{
	Serializable, Stream,
	Reader, Error as ReaderError
};
use chain::{Transaction, Block};
use common::Command;
use types::{
	Version, Addr, AddrBelow31402, Inv,
	GetData, NotFound, GetBlocks, GetHeaders
};

pub fn deserialize_payload(data: &[u8], version: u32, command: &Command) -> Result<Payload, ReaderError> {
	let mut reader = Reader::new(data);
	match &command.to_string() as &str {
		"version" => reader.read().map(Payload::Version),
		"verack" => Ok(Payload::Verack),
		"addr" => match version >= 31402 {
			true => reader.read().map(Payload::Addr),
			false => reader.read().map(Payload::AddrBelow31402),
		},
		"inv" => reader.read().map(Payload::Inv),
		"getdata" => reader.read().map(Payload::GetData),
		"notfound" => reader.read().map(Payload::NotFound),
		"getblocks" => reader.read().map(Payload::GetBlocks),
		"getheaders" => reader.read().map(Payload::GetHeaders),
		"tx" => reader.read().map(Payload::Tx),
		"block" => reader.read().map(Payload::Block),
		_ => Err(ReaderError::MalformedData),
	}
}

#[derive(Debug, PartialEq)]
pub enum Payload {
	Version(Version),
	Verack,
	Addr(Addr),
	AddrBelow31402(AddrBelow31402),
	Inv(Inv),
	GetData(GetData),
	NotFound(NotFound),
	GetBlocks(GetBlocks),
	GetHeaders(GetHeaders),
	Tx(Transaction),
	Block(Block),
}

impl Payload {
	pub fn command(&self) -> Command {
		let cmd = match *self {
			Payload::Version(_) => "version",
			Payload::Verack => "verack",
			Payload::Addr(_) | Payload::AddrBelow31402(_) => "addr",
			Payload::Inv(_) => "inv",
			Payload::GetData(_) => "getdata",
			Payload::NotFound(_) => "notfound",
			Payload::GetBlocks(_) => "getblocks",
			Payload::GetHeaders(_) => "getheaders",
			Payload::Tx(_) => "tx",
			Payload::Block(_) => "block",
		};

		cmd.into()
	}

}

impl Serializable for Payload {
	fn serialize(&self, stream: &mut Stream) {
		match *self {
			Payload::Version(ref p) => { stream.append(p); },
			Payload::Verack => {},
			Payload::Addr(ref p) => { stream.append(p); },
			Payload::AddrBelow31402(ref p) => { stream.append(p); },
			Payload::Inv(ref p) => { stream.append(p); },
			Payload::GetData(ref p) => { stream.append(p); },
			Payload::NotFound(ref p) => { stream.append(p); },
			Payload::GetBlocks(ref p) => { stream.append(p); },
			Payload::GetHeaders(ref p) => { stream.append(p); },
			Payload::Tx(ref p) => { stream.append(p); },
			Payload::Block(ref p) => { stream.append(p); },
		}
	}
}
