use bytes::Bytes;
use ser::{
	Serializable, Stream,
	Deserializable, Reader, Error as ReaderError, deserialize
};
use common::{NetAddress, ServiceFlags};
use serialization::PayloadType;

#[derive(Debug, PartialEq)]
pub enum Version {
	V0(V0),
	V106(V0, V106),
	V70001(V0, V106, V70001),
}

impl PayloadType for Version {
	fn version() -> u32 {
		0
	}

	fn command() -> &'static str {
		"version"
	}
}

impl Version {
	pub fn version(&self) -> u32 {
		match *self {
			Version::V0(ref s) => s.version,
			Version::V106(ref s, _) => s.version,
			Version::V70001(ref s, _, _) => s.version,
		}
	}
}

#[derive(Debug, PartialEq)]
pub struct V0 {
	pub version: u32,
	pub services: ServiceFlags,
	pub timestamp: i64,
	pub receiver: NetAddress,
}

#[derive(Debug, PartialEq)]
pub struct V106 {
	pub from: NetAddress,
	pub nonce: u64,
	pub user_agent: String,
	pub start_height: i32,
}

#[derive(Debug, PartialEq)]
pub struct V70001 {
	pub relay: bool,
}

impl Serializable for Version {
	fn serialize(&self, stream: &mut Stream) {
		match *self {
			Version::V0(ref simple) => {
				stream.append(simple);
			},
			Version::V106(ref simple, ref v106) => {
				stream
					.append(simple)
					.append(v106);
			},
			Version::V70001(ref simple, ref v106, ref v70001) => {
				stream
					.append(simple)
					.append(v106)
					.append(v70001);
			},
		}
	}
}

impl Deserializable for Version {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let simple: V0 = try!(reader.read());

		if simple.version < 106 {
			return Ok(Version::V0(simple));
		}

		let v106: V106 = try!(reader.read());
		if simple.version < 70001 {
			Ok(Version::V106(simple, v106))
		} else {
			let v70001: V70001 = try!(reader.read());
			Ok(Version::V70001(simple, v106, v70001))
		}
	}
}

impl Serializable for V0 {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.version)
			.append(&self.services)
			.append(&self.timestamp)
			.append(&self.receiver);
	}
}

impl Deserializable for V0 {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let result = V0 {
			version: try!(reader.read()),
			services: try!(reader.read()),
			timestamp: try!(reader.read()),
			receiver: try!(reader.read()),
		};

		Ok(result)
	}
}

impl Serializable for V106 {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.from)
			.append(&self.nonce)
			.append(&self.user_agent)
			.append(&self.start_height);
	}
}

impl Deserializable for V106 {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let result = V106 {
			from: try!(reader.read()),
			nonce: try!(reader.read()),
			user_agent: try!(reader.read()),
			start_height: try!(reader.read()),
		};

		Ok(result)
	}
}

impl Serializable for V70001 {
	fn serialize(&self, stream: &mut Stream) {
		stream.append(&self.relay);
	}
}

impl Deserializable for V70001 {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let result = V70001 {
			relay: try!(reader.read()),
		};

		Ok(result)
	}
}

impl From<&'static str> for Version {
	fn from(s: &'static str) -> Self {
		let bytes: Bytes = s.into();
		deserialize(&bytes).unwrap()
	}
}

#[cfg(test)]
mod test {
	use bytes::Bytes;
	use ser::{serialize, deserialize};
	use super::{Version, V0, V106};

	#[test]
	fn test_version_serialize() {
		let expected: Bytes = "9c7c00000100000000000000e615104d00000000010000000000000000000000000000000000ffff0a000001208d010000000000000000000000000000000000ffff0a000002208ddd9d202c3ab457130055810100".into();

		let version = Version::V106(V0 {
			version: 31900,
			services: 1u64.into(),
			timestamp: 0x4d1015e6,
			receiver: "010000000000000000000000000000000000ffff0a000001208d".into(),
		}, V106 {
			from: "010000000000000000000000000000000000ffff0a000002208d".into(),
			nonce: 0x1357b43a2c209ddd,
			user_agent: "".into(),
			start_height: 98645,
		});

		assert_eq!(serialize(&version), expected);
	}

	#[test]
	fn test_version_deserialize() {
		let raw: Bytes = "9c7c00000100000000000000e615104d00000000010000000000000000000000000000000000ffff0a000001208d010000000000000000000000000000000000ffff0a000002208ddd9d202c3ab457130055810100".into();

		let expected = Version::V106(V0 {
			version: 31900,
			services: 1u64.into(),
			timestamp: 0x4d1015e6,
			receiver: "010000000000000000000000000000000000ffff0a000001208d".into(),
		}, V106 {
			from: "010000000000000000000000000000000000ffff0a000002208d".into(),
			nonce: 0x1357b43a2c209ddd,
			user_agent: "".into(),
			start_height: 98645,
		});

		assert_eq!(expected, deserialize(&raw).unwrap());
	}
}
