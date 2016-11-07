use std::io;
use chain::BlockHeader;
use ser::{Stream, Reader, Serializable, Deserializable, CompactInteger, Error as ReaderError};
use {Payload, MessageResult};

#[derive(Debug, PartialEq)]
pub struct Headers {
	// TODO: Block headers need to have txn_count field
	pub headers: Vec<BlockHeader>,
}

#[derive(Debug, PartialEq)]
struct HeaderWithTxnCount {
	pub header: BlockHeader,
	pub txn_count: u64,
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
			headers: headers_with_txn_count.into_iter().map(|h| h.header).collect(),
		};

		Ok(headers)
	}

	fn serialize_payload(&self, stream: &mut Stream, _version: u32) -> MessageResult<()> {
		let headers_with_txn_count: Vec<_> = self.headers.iter().map(|h| HeaderWithTxnCount { header: h.clone(), txn_count: 0 }).collect();
		stream.append_list(&headers_with_txn_count);
		Ok(())
	}
}

impl Serializable for HeaderWithTxnCount {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.header)
			.append(&CompactInteger::from(0u32));
	}
}

impl Deserializable for HeaderWithTxnCount {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let header = HeaderWithTxnCount {
			header: try!(reader.read()),
			txn_count: try!(reader.read::<CompactInteger>()).into(),
		};

		Ok(header)
	}
}
