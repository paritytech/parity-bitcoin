use std::io;
use ser::{Stream, Reader};
use common::InventoryVector;
use {Payload, MessageResult};

pub const GETDATA_MAX_INVENTORY_LEN: usize = 50_000;

#[derive(Debug, PartialEq)]
pub struct GetData {
	pub inventory: Vec<InventoryVector>,
}

impl GetData {
	pub fn with_inventory(inventory: Vec<InventoryVector>) -> Self {
		GetData {
			inventory: inventory,
		}
	}
}

impl Payload for GetData {
	fn version() -> u32 {
		0
	}

	fn command() -> &'static str {
		"getdata"
	}

	fn deserialize_payload<T>(reader: &mut Reader<T>, _version: u32) -> MessageResult<Self> where T: io::Read {
		let inv = GetData {
			inventory: try!(reader.read_list_max(50_000)),
		};

		Ok(inv)
	}

	fn serialize_payload(&self, stream: &mut Stream, _version: u32) -> MessageResult<()> {
		stream.append_list(&self.inventory);
		Ok(())
	}
}
