use std::fmt;
use serde::{Serialize, Serializer, Deserialize, Deserializer};
use serde::de::Unexpected;
use p2p::{Direction, PeerInfo};

#[derive(Debug, PartialEq)]
pub enum AddNodeOperation {
	Add,
	Remove,
	OneTry,
}

impl<'a> Deserialize<'a> for AddNodeOperation {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: Deserializer<'a> {
		use serde::de::Visitor;

		struct DummyVisitor;

		impl<'b> Visitor<'b> for DummyVisitor {
			type Value = AddNodeOperation;

			fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
				formatter.write_str("a node operation string")
			}

			fn visit_str<E>(self, value: &str) -> Result<AddNodeOperation, E> where E: ::serde::de::Error {
				match value {
					"add" => Ok(AddNodeOperation::Add),
					"remove" => Ok(AddNodeOperation::Remove),
					"onetry" => Ok(AddNodeOperation::OneTry),
					_ => Err(E::invalid_value(Unexpected::Str(value), &self)),
				}
			}
		}

		deserializer.deserialize_identifier(DummyVisitor)
	}
}

#[derive(Serialize)]
pub struct NodeInfoAddress {
	address: String,
	connected: NodeInfoAddressConnectionType,
}

impl From<PeerInfo> for NodeInfoAddress {
	fn from(info: PeerInfo) -> Self {
		NodeInfoAddress {
			address: format!("{}", info.address),
			connected: match info.direction {
				Direction::Inbound => NodeInfoAddressConnectionType::Inbound,
				Direction::Outbound => NodeInfoAddressConnectionType::Outbound,
			},
		}
	}
}

#[derive(Serialize)]
pub struct NodeInfo {
	pub addednode: String,
	pub connected: bool,
	pub addresses: Vec<NodeInfoAddress>,
}

pub enum NodeInfoAddressConnectionType {
	Inbound,
	Outbound,
}

impl Serialize for NodeInfoAddressConnectionType {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
		match *self {
			NodeInfoAddressConnectionType::Inbound => "inbound".serialize(serializer),
			NodeInfoAddressConnectionType::Outbound => "outbound".serialize(serializer),
		}
	}
}
