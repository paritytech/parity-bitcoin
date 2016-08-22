use std::fmt;
use script::Opcode;

#[derive(Debug, PartialEq)]
pub enum Error {
	// Logical/Format/Canonical errors.
	BadOpcode(u8),
	DisabledOpcode(Opcode),

	// BIP62
	SignatureHashtype,
	SignatureDer,
	SignatureHighS,
	PubkeyType,
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match *self {
			// Logical/Format/Canonical errors.
			Error::BadOpcode(opcode) => writeln!(f, "Bad Opcode: {}", opcode),
			Error::DisabledOpcode(ref opcode) => writeln!(f, "Disabled Opcode: {:?}", opcode),

			// BIP62
			Error::SignatureHashtype => "Invalid Signature Hashtype".fmt(f),
			Error::SignatureDer => "Invalid Signature".fmt(f),
			Error::SignatureHighS => "Invalid High S in Signature".fmt(f),
			Error::PubkeyType => "Invalid Pubkey".fmt(f),
		}
	}
}
