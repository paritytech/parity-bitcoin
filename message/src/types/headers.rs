use std::io;
use chain::BlockHeader;
use ser::{Stream, Reader, Serializable, Deserializable, CompactInteger, Error as ReaderError};
use {Payload, MessageResult};

pub const HEADERS_MAX_HEADERS_LEN: usize = 2000;

#[derive(Debug, PartialEq)]
pub struct Headers {
	pub headers: Vec<BlockHeader>,
}

impl Headers {
	pub fn with_headers(headers: Vec<BlockHeader>) -> Self {
		Headers {
			headers: headers,
		}
	}
}

#[derive(Debug, PartialEq)]
struct HeaderWithTxnCount {
	header: BlockHeader,
}

impl From<HeaderWithTxnCount> for BlockHeader {
	fn from(header: HeaderWithTxnCount) -> BlockHeader {
		header.header
	}
}

#[derive(Debug, PartialEq)]
struct HeaderWithTxnCountRef<'a> {
	header: &'a BlockHeader,
}

impl<'a> From<&'a BlockHeader> for HeaderWithTxnCountRef<'a> {
	fn from(header: &'a BlockHeader) -> Self {
		HeaderWithTxnCountRef {
			header: header,
		}
	}
}

impl Payload for Headers {
	fn version() -> u32 {
		0
	}

	fn command() -> &'static str {
		"headers"
	}

	fn deserialize_payload<T>(reader: &mut Reader<T>, _version: u32) -> MessageResult<Self> where T: io::Read {
		let headers_with_txn_count: Vec<HeaderWithTxnCount> = try!(reader.read_list());
		let headers = Headers {
			headers: headers_with_txn_count.into_iter().map(Into::into).collect(),
		};

		Ok(headers)
	}

	fn serialize_payload(&self, stream: &mut Stream, _version: u32) -> MessageResult<()> {
		let headers_with_txn_count: Vec<HeaderWithTxnCountRef> = self.headers.iter().map(Into::into).collect();
		stream.append_list(&headers_with_txn_count);
		Ok(())
	}
}

impl<'a> Serializable for HeaderWithTxnCountRef<'a> {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(self.header)
			.append(&CompactInteger::from(0u32));
	}
}

impl Deserializable for HeaderWithTxnCount {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let header = HeaderWithTxnCount {
			header: try!(reader.read()),
		};

		let txn_count: CompactInteger = try!(reader.read());
		if txn_count != 0u32.into() {
			return Err(ReaderError::MalformedData);
		}

		Ok(header)
	}
}
