use std::io;
use ser::{Stream, Reader};
use common::InventoryVector;
use {Payload, MessageResult};

#[derive(Debug, PartialEq)]
pub struct NotFound {
	pub inventory: Vec<InventoryVector>,
}

impl NotFound {
	pub fn with_inventory(inventory: Vec<InventoryVector>) -> Self {
		NotFound {
			inventory: inventory,
		}
	}
}

impl Payload for NotFound {
	fn version() -> u32 {
		0
	}

	fn command() -> &'static str {
		"notfound"
	}

	fn deserialize_payload<T>(reader: &mut Reader<T>, _version: u32) -> MessageResult<Self> where T: io::Read {
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
