use std::fmt;
use script::Opcode;

#[derive(Debug, PartialEq)]
pub enum Error {
	DisabledOpcode(Opcode),
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match *self {
			Error::DisabledOpcode(ref opcode) => writeln!(f, "Disabled Opcode: {:?}", opcode),
		}
	}
}
