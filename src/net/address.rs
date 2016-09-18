use std::{net, str};
use byteorder::{BigEndian, WriteBytesExt, ReadBytesExt};
use stream::{Stream, Serializable};
use reader::{Reader, Deserializable, Error as ReaderError};
use net::ServiceFlags;

#[derive(Debug, PartialEq)]
pub struct Port(u16);

impl From<u16> for Port {
	fn from(port: u16) -> Self {
		Port(port)
	}
}

impl Serializable for Port {
	fn serialize(&self, stream: &mut Stream) {
		stream.write_u16::<BigEndian>(self.0).unwrap();
	}
}

impl Deserializable for Port {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		Ok(try!(reader.read_u16::<BigEndian>().map(Port)))
	}
}

#[derive(Debug, PartialEq)]
pub struct IpAddress(net::IpAddr);

impl From<net::IpAddr> for IpAddress {
	fn from(ip: net::IpAddr) -> Self {
		IpAddress(ip)
	}
}

impl From<&'static str> for IpAddress {
	fn from(s: &'static str) -> Self {
		s.parse().unwrap()
	}
}

impl str::FromStr for IpAddress {
	type Err = <net::IpAddr as str::FromStr>::Err;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		s.parse().map(IpAddress)
	}
}

impl Serializable for IpAddress {
	fn serialize(&self, stream: &mut Stream) {
		match self.0 {
			net::IpAddr::V4(address) => {
				stream
					.append_bytes(&[0u8; 12])
					.append_bytes(&address.octets());
			},
			net::IpAddr::V6(address) => {
				for segment in &address.segments() {
					stream.write_u16::<BigEndian>(*segment).unwrap();
				}
			},
		}
	}
}

impl Deserializable for IpAddress {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let mut bytes = try!(reader.read_bytes(12));
		if bytes == &[0u8; 12] {
			let address = try!(reader.read_bytes(4));
			let address = net::Ipv4Addr::new(address[0], address[1], address[2], address[3]);
			Ok(IpAddress(net::IpAddr::V4(address)))
		} else {
			let address = net::Ipv6Addr::new(
				try!(bytes.read_u16::<BigEndian>()),
				try!(bytes.read_u16::<BigEndian>()),
				try!(bytes.read_u16::<BigEndian>()),
				try!(bytes.read_u16::<BigEndian>()),
				try!(bytes.read_u16::<BigEndian>()),
				try!(bytes.read_u16::<BigEndian>()),
				try!(reader.read_u16::<BigEndian>()),
				try!(reader.read_u16::<BigEndian>())
			);
			Ok(IpAddress(net::IpAddr::V6(address)))
		}
	}
}

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
	use stream::serialize;
	use reader::deserialize;
	use net::ServiceFlags;
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
