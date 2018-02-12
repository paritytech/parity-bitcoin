use std::{str, fmt};
use hash::H96;
use Error;

#[derive(Debug, PartialEq, Clone, Hash, Eq, Serializable, Deserializable)]
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

impl Command {
	pub fn len(&self) -> usize {
		let trailing_zeros = self.0.iter().rev().take_while(|&x| x == &0).count();
		self.0.len() - trailing_zeros
	}

	pub fn is_empty(&self) -> bool {
		self.0.is_zero()
	}

	fn as_string(&self) -> String {
		String::from_utf8_lossy(&self.0[..self.len()]).to_ascii_lowercase()
	}
}

impl From<Command> for String {
	fn from(c: Command) -> Self {
		c.as_string()
	}
}

impl fmt::Display for Command {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.write_str(&self.as_string())
	}
}

impl<'a> PartialEq<&'a str> for Command {
	fn eq(&self, other: &&'a str) -> bool {
		self.len() == other.len() &&
		&self.0[..other.len()] == other.as_ref() as &[u8]
	}
}

#[cfg(test)]
mod tests {
	use bytes::Bytes;
	use ser::{serialize, deserialize};
	use super::Command;

	#[test]
	fn test_command_parse() {
		let command: Command = "version".into();
		assert_eq!(Command("76657273696f6e0000000000".into()), command);
	}

	#[test]
	fn test_command_to_string() {
		let command: Command = "version".into();
		let expected: String = "version".into();
		assert_eq!(expected, String::from(command));
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

		assert_eq!(expected, deserialize::<_, Command>(raw.as_ref()).unwrap());
	}

	#[test]
	fn partial_eq_command_str() {
		let command: Command = "version".into();
		assert_eq!(command, "version");
		assert!(command != "ver");
		assert!(command != "versionx");
	}

}
