use ser::{Stream, Reader};
use common::InventoryVector;
use {Payload, MessageResult};

#[derive(Debug, PartialEq)]
pub struct NotFound {
	pub inventory: Vec<InventoryVector>,
}

impl Payload for NotFound {
	fn version() -> u32 {
		0
	}

	fn command() -> &'static str {
		"notfound"
	}

	fn deserialize_payload(reader: &mut Reader, _version: u32) -> MessageResult<Self> where Self: Sized {
		let inv = NotFound {
			inventory: try!(reader.read_list_max(50_000)),
		};

		Ok(inv)
	}

	fn serialize_payload(&self, stream: &mut Stream, _version: u32) -> MessageResult<()> {
		stream.append_list(&self.inventory);
		Ok(())
	}
}
