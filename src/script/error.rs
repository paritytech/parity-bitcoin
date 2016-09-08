use std::fmt;
use script::Opcode;

#[derive(Debug, PartialEq)]
pub enum Error {
	Unknown,
	ReturnOpcode,

	// Max sizes.
	ScriptSize,
	PushSize,
	NumberOverflow,
	NumberNotMinimallyEncoded,

	// Failed verify operations
	Verify,
	EqualVerify,

	// Logical/Format/Canonical errors.
	BadOpcode,
	DisabledOpcode(Opcode),
	InvalidStackOperation,

	// CHECKLOCKTIMEVERIFY and CHECKSEQUENCEVERIFY
	NegativeLocktime,
	UnsatisfiedLocktime,

	// BIP62
	SignatureHashtype,
	SignatureDer,
	Minimaldata,
	SignatureHighS,
	PubkeyType,

	// Softfork safeness
	DiscourageUpgradableNops,
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match *self {
			Error::Unknown => "Unknown error".fmt(f),
			Error::ReturnOpcode => "Used return opcode".fmt(f),

			// Failed verify operations
			Error::Verify => "Failed verify operation".fmt(f),
			Error::EqualVerify => "Failed equal verify operation".fmt(f),

			// Max sizes.
			Error::ScriptSize => "Script is too long".fmt(f),
			Error::PushSize => "Pushing too many bytes".fmt(f),
			Error::NumberOverflow => "Number overflow".fmt(f),
			Error::NumberNotMinimallyEncoded => "Number not minimally encoded".fmt(f),

			// Logical/Format/Canonical errors.
			Error::BadOpcode => "Bad Opcode".fmt(f),
			Error::DisabledOpcode(ref opcode) => writeln!(f, "Disabled Opcode: {:?}", opcode),
			Error::InvalidStackOperation => "Invalid stack operation".fmt(f),

			Error::NegativeLocktime => "Negative locktime".fmt(f),
			Error::UnsatisfiedLocktime => "UnsatisfiedLocktime".fmt(f),

			// BIP62
			Error::SignatureHashtype => "Invalid Signature Hashtype".fmt(f),
			Error::SignatureDer => "Invalid Signature".fmt(f),
			Error::Minimaldata => "Check minimaldata failed".fmt(f),
			Error::SignatureHighS => "Invalid High S in Signature".fmt(f),
			Error::PubkeyType => "Invalid Pubkey".fmt(f),

			// Softfork safeness
			Error::DiscourageUpgradableNops => "Discourage Upgradable Nops".fmt(f),
		}
	}
}
