use std::fmt;
use script::Opcode;

#[derive(Debug, PartialEq)]
pub enum Error {
	// Max sizes.
	ScriptSize,
	PushSize,

	// Logical/Format/Canonical errors.
	BadOpcode,
	DisabledOpcode(Opcode),

	// BIP62
	SignatureHashtype,
	SignatureDer,
	Minimaldata,
	SignatureHighS,
	PubkeyType,
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match *self {
			// Max sizes.
			Error::ScriptSize => "Script is too long".fmt(f),
			Error::PushSize => "Pushing too many bytes".fmt(f),

			// Logical/Format/Canonical errors.
			Error::BadOpcode => "Bad Opcode".fmt(f),
			Error::DisabledOpcode(ref opcode) => writeln!(f, "Disabled Opcode: {:?}", opcode),

			// BIP62
			Error::SignatureHashtype => "Invalid Signature Hashtype".fmt(f),
			Error::SignatureDer => "Invalid Signature".fmt(f),
			Error::Minimaldata => "Check minimaldata failed".fmt(f),
			Error::SignatureHighS => "Invalid High S in Signature".fmt(f),
			Error::PubkeyType => "Invalid Pubkey".fmt(f),
		}
	}
}
