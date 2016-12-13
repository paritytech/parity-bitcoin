use serde::{Serialize, Serializer, Deserialize, Deserializer};

#[derive(Debug, PartialEq)]
pub enum AddNodeOperation {
	Add,
	Remove,
	OneTry,
}

impl Deserialize for AddNodeOperation {
	fn deserialize<D>(deserializer: &mut D) -> Result<Self, D::Error> where D: Deserializer {
		use serde::de::Visitor;

		struct DummyVisitor;

		impl Visitor for DummyVisitor {
			type Value = AddNodeOperation;

			fn visit_str<E>(&mut self, value: &str) -> Result<AddNodeOperation, E> where E: ::serde::de::Error {
				match value {
					"add" => Ok(AddNodeOperation::Add),
					"remove" => Ok(AddNodeOperation::Remove),
					"onetry" => Ok(AddNodeOperation::OneTry),
					_ => Err(E::invalid_value(&format!("unknown ScriptType variant: {}", value))),
				}
			}
		}

		deserializer.deserialize(DummyVisitor)
	}
}

#[derive(Serialize)]
pub struct NodeInfoAddress {
	address: String,
	connected: NodeInfoAddressConnectionType,
}

#[derive(Serialize)]
pub struct NodeInfo {
	addednode: String,
	connected: bool,
	addresses: Vec<NodeInfoAddress>,
}

pub enum NodeInfoAddressConnectionType {
	Inbound,
	Outbound,
}

impl Serialize for NodeInfoAddressConnectionType {
	fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error> where S: Serializer {
		match *self {
			NodeInfoAddressConnectionType::Inbound => "inbound".serialize(serializer),
			NodeInfoAddressConnectionType::Outbound => "outbound".serialize(serializer),
		}
	}
}
