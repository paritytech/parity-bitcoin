use std::io;
use bytes::Bytes;
use ser::{
	Stream, Serializable,
	Reader, Deserializable, Error as ReaderError, deserialize,
};
use common::{Port, IpAddress, Services};

#[derive(Debug, PartialEq)]
pub struct NetAddress {
	pub services: Services,
	pub address: IpAddress,
	pub port: Port,
}

impl Serializable for NetAddress {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.services)
			.append(&self.address)
			.append(&self.port);
	}
}

impl Deserializable for NetAddress {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let net = NetAddress {
			services: try!(reader.read()),
			address: try!(reader.read()),
			port: try!(reader.read()),
		};
		Ok(net)
	}
}

impl From<&'static str> for NetAddress {
	fn from(s: &'static str) -> Self {
		let bytes: Bytes = s.into();
		deserialize(bytes.as_ref()).unwrap()
	}
}

#[cfg(test)]
mod tests {
	use ser::{serialize, deserialize};
	use common::Services;
	use super::NetAddress;

	#[test]
	fn test_net_address_serialize() {
		let expected = vec![
			0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0x0a, 0x00, 0x00, 0x01,
			0x20, 0x8d
		].into();

		let address = NetAddress {
			services: Services::default().with_network(true),
			address: "::ffff:a00:1".into(),
			port: 8333.into(),
		};

		assert_eq!(serialize(&address), expected);
	}

	#[test]
	fn test_net_address_deserialize() {
		let bytes = vec![
			0x01u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0x0a, 0x00, 0x00, 0x01,
			0x20, 0x8d
		];

		let expected = NetAddress {
			services: Services::default().with_network(true),
			address: "::ffff:a00:1".into(),
			port: 8333.into(),
		};

		assert_eq!(expected, deserialize(&bytes as &[u8]).unwrap());
	}

	#[test]
	fn test_net_address_from_static_str() {
		let expected = NetAddress {
			services: Services::default().with_network(true),
			address: "::ffff:a00:1".into(),
			port: 8333.into(),

		};
		let s = "010000000000000000000000000000000000ffff0a000001208d";
		assert_eq!(expected, s.into());
	}
}
