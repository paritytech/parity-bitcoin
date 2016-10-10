use ser::{Stream, Reader};
use common::InventoryVector;
use {PayloadType, MessageResult};

#[derive(Debug, PartialEq)]
pub struct Inv {
	pub inventory: Vec<InventoryVector>,
}

impl PayloadType for Inv {
	fn version() -> u32 {
		0
	}

	fn command() -> &'static str {
		"inv"
	}

	fn deserialize_payload(reader: &mut Reader, _version: u32) -> MessageResult<Self> where Self: Sized {
		let inv = Inv {
			inventory: try!(reader.read_list_max(50_000)),
		};

		Ok(inv)
	}

	fn serialize_payload(&self, stream: &mut Stream, _version: u32) -> MessageResult<()> {
		stream.append_list(&self.inventory);
		Ok(())
	}
}
