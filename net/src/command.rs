use std::str;
use std::ascii::AsciiExt;
use hash::H96;
use ser::{Serializable, Stream, Deserializable, Reader, Error as ReaderError};
use Error;

#[derive(Debug, PartialEq)]
pub struct Command(H96);

impl str::FromStr for Command {
	type Err = Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		if !s.is_ascii() || s.len() > 12 {
			return Err(Error::InvalidCommand);
		}

		let mut result = H96::default();
		result[..s.len()].copy_from_slice(s.as_ref());
		Ok(Command(result))
	}
}

impl From<&'static str> for Command {
	fn from(s: &'static str) -> Self {
		s.parse().unwrap()
	}
}

impl Serializable for Command {
	fn serialize(&self, stream: &mut Stream) {
		stream.append(&self.0);
	}
}

impl Deserializable for Command {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		reader.read().map(Command)
	}
}

#[cfg(test)]
mod tests {
	use bytes::Bytes;
	use ser::{serialize, deserialize};
	use super::Command;

	#[test]
	fn test_parse() {
		assert_eq!(Command("76657273696f6e0000000000".into()), "version".into());
	}

	#[test]
	fn test_command_serialize() {
		let expected = "76657273696f6e0000000000".into();
		let command: Command = "version".into();

		assert_eq!(serialize(&command), expected);
	}

	#[test]
	fn test_command_deserialize() {
		let raw: Bytes = "76657273696f6e0000000000".into();
		let expected: Command = "version".into();

		assert_eq!(expected, deserialize(&raw).unwrap());
	}

}
