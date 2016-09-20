use crypto::checksum;
use ser::{
	Serializable, Stream,
	Deserializable, Reader, Error as ReaderError
};
use common::Command;

#[derive(Debug, PartialEq)]
pub struct MessageHeader {
	pub magic: u32,
	pub command: Command,
	pub len: u32,
	pub checksum: [u8; 4],
}

impl Deserializable for MessageHeader {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let header = MessageHeader {
			magic: try!(reader.read()),
			command: try!(reader.read()),
			len: try!(reader.read()),
			checksum: {
				let slice = try!(reader.read_slice(4));
				let mut checksum = [0u8; 4];
				checksum.copy_from_slice(slice);
				checksum
			},
		};

		Ok(header)
	}
}

impl Serializable for MessageHeader {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.magic)
			.append(&self.command)
			.append(&self.len)
			.append_slice(&self.checksum);
	}
}

