use serde::{Deserialize, Deserializer};

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
