use ser::{
	Stream, Serializable,
	Reader, Deserializable, Error as ReaderError
};
use {Port, IpAddress, ServiceFlags};

#[derive(Debug, PartialEq)]
pub struct NetAddress {
	pub services: ServiceFlags,
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
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let net = NetAddress {
			services: try!(reader.read()),
			address: try!(reader.read()),
			port: try!(reader.read()),
		};
		Ok(net)
	}
}

#[cfg(test)]
mod tests {
	use ser::{serialize, deserialize};
	use ServiceFlags;
	use super::NetAddress;

	#[test]
	fn test_net_address_serialize() {
		let expeted = vec![
			0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0x0a, 0x00, 0x00, 0x01,
			0x20, 0x8d
		];

		let address = NetAddress {
			services: ServiceFlags::default().with_network(true),
			address: "::ffff:a00:1".into(),
			port: 8333.into(),
		};

		assert_eq!(expeted, serialize(&address));
	}

	#[test]
	fn test_net_address_deserialize() {
		let bytes = vec![
			0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0x0a, 0x00, 0x00, 0x01,
			0x20, 0x8d
		];

		let expected = NetAddress {
			services: ServiceFlags::default().with_network(true),
			address: "::ffff:a00:1".into(),
			port: 8333.into(),
		};

		assert_eq!(expected, deserialize(&bytes).unwrap());
	}
}
