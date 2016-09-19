use bytes::Bytes;
use crypto::checksum;
use ser::{
	Serializable, Stream, CompactInteger,
	Deserializable, Reader, Error as ReaderError
};
use Command;

#[derive(Debug, PartialEq)]
pub struct Message {
	pub magic: u32,
	pub command: Command,
	pub payload: Bytes,
}

impl Message {
	#[inline]
	pub fn payload_checksum(&self) -> [u8; 4] {
		checksum(&self.payload)
	}
}

impl Serializable for Message {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.magic)
			.append(&self.command)
			.append(&CompactInteger::from(self.payload.len()))
			.append_slice(&self.payload_checksum())
			.append_slice(&self.payload);
	}
}

impl Deserializable for Message {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let magic = try!(reader.read());
		let command = try!(reader.read());
		let len: CompactInteger = try!(reader.read());
		let cs = try!(reader.read_slice(4));
		let payload = try!(reader.read_slice(len.into()));

		if cs != &checksum(payload) {
			return Err(ReaderError::MalformedData);
		}

		let message = Message {
			magic: magic,
			command: command,
			payload: payload.to_vec().into(),
		};

		Ok(message)
	}
}

#[cfg(test)]
mod tests {
}
