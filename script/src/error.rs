//! Interpreter errors

use std::fmt;
use Opcode;

/// Interpreter errors
#[derive(Debug, PartialEq)]
pub enum Error {
	Unknown,
	EvalFalse,
	ReturnOpcode,

	// Max sizes.
	ScriptSize,
	PushSize,
	OpCount,
	StackSize,
	NumberOverflow,
	NumberNotMinimallyEncoded,
	SigCount,
	PubkeyCount,
	InvalidOperandSize,

	// Failed verify operations
	Verify,
	EqualVerify,
	CheckSigVerify,
	CheckDataSigVerify,
	NumEqualVerify,

	// Logical/Format/Canonical errors.
	BadOpcode,
	DisabledOpcode(Opcode),
	InvalidStackOperation,
	InvalidAltstackOperation,
	UnbalancedConditional,
	InvalidSplitRange,
	DivisionByZero,
	ImpossibleEncoding,

	// CHECKLOCKTIMEVERIFY and CHECKSEQUENCEVERIFY
	NegativeLocktime,
	UnsatisfiedLocktime,

	// BIP62
	SignatureHashtype,
	SignatureDer,
	SignatureIllegalForkId,
	SignatureMustUseForkId,
	Minimaldata,
	SignaturePushOnly,
	SignatureHighS,
	SignatureNullDummy,
	PubkeyType,
	Cleanstack,

	// Softfork safeness
	DiscourageUpgradableNops,
	DiscourageUpgradableWitnessProgram,

	// SegWit-related errors
	WitnessProgramWrongLength,
	WitnessProgramWitnessEmpty,
	WitnessProgramMismatch,
	WitnessMalleated,
	WitnessMalleatedP2SH,
	WitnessUnexpected,
	WitnessPubKeyType,
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match *self {
			Error::Unknown => "Unknown error".fmt(f),
			Error::EvalFalse => "Script evaluated to false".fmt(f),
			Error::ReturnOpcode => "Used return opcode".fmt(f),

			// Failed verify operations
			Error::Verify => "Failed verify operation".fmt(f),
			Error::EqualVerify => "Failed equal verify operation".fmt(f),
			Error::CheckSigVerify => "Failed signature check".fmt(f),
			Error::CheckDataSigVerify => "Failed data signature check".fmt(f),
			Error::NumEqualVerify => "Failed num equal verify operation".fmt(f),
			Error::SigCount => "Maximum number of signature exceeded".fmt(f),
			Error::PubkeyCount => "Maximum number of pubkeys per multisig exceeded".fmt(f),

			// Max sizes.
			Error::ScriptSize => "Script is too long".fmt(f),
			Error::PushSize => "Pushing too many bytes".fmt(f),
			Error::OpCount => "Script contains to many opcodes".fmt(f),
			Error::StackSize => "Stack is too big".fmt(f),
			Error::NumberOverflow => "Number overflow".fmt(f),
			Error::NumberNotMinimallyEncoded => "Number not minimally encoded".fmt(f),

			// Logical/Format/Canonical errors.
			Error::BadOpcode => "Bad Opcode".fmt(f),
			Error::DisabledOpcode(ref opcode) => writeln!(f, "Disabled Opcode: {:?}", opcode),
			Error::InvalidStackOperation => "Invalid stack operation".fmt(f),
			Error::InvalidAltstackOperation => "Invalid altstack operation".fmt(f),
			Error::UnbalancedConditional => "Unbalanced conditional".fmt(f),
			Error::InvalidSplitRange => "Invalid OP_SPLIT range".fmt(f),
			Error::InvalidOperandSize => "Invalid operand size".fmt(f),
			Error::DivisionByZero => "Invalid division operation".fmt(f),
			Error::ImpossibleEncoding => "The requested encoding is impossible to satisfy".fmt(f),

			// CHECKLOCKTIMEVERIFY and CHECKSEQUENCEVERIFY
			Error::NegativeLocktime => "Negative locktime".fmt(f),
			Error::UnsatisfiedLocktime => "UnsatisfiedLocktime".fmt(f),

			// BIP62
			Error::SignatureHashtype => "Invalid Signature Hashtype".fmt(f),
			Error::SignatureDer => "Invalid Signature".fmt(f),
			Error::SignatureIllegalForkId => "Illegal use of SIGHASH_FORKID".fmt(f),
			Error::SignatureMustUseForkId => "Signature must use SIGHASH_FORKID".fmt(f),
			Error::Minimaldata => "Check minimaldata failed".fmt(f),
			Error::SignaturePushOnly => "Only push opcodes are allowed in this signature".fmt(f),
			Error::SignatureHighS => "Invalid High S in Signature".fmt(f),
			Error::SignatureNullDummy => "Multisig extra stack element is not empty".fmt(f),
			Error::PubkeyType => "Invalid Pubkey".fmt(f),
			Error::Cleanstack => "Only one element is expected to remain at stack at the end of execution".fmt(f),

			// Softfork safeness
			Error::DiscourageUpgradableNops => "Discourage Upgradable Nops".fmt(f),
			Error::DiscourageUpgradableWitnessProgram => "Discourage Upgradable Witness Program".fmt(f),

			// SegWit-related errors
			Error::WitnessProgramWrongLength => "Witness program has incorrect length".fmt(f),
			Error::WitnessProgramWitnessEmpty => "Witness program was passed an empty witness".fmt(f),
			Error::WitnessProgramMismatch => "Witness program hash mismatch".fmt(f),
			Error::WitnessMalleated => "Witness requires empty scriptSig".fmt(f),
			Error::WitnessMalleatedP2SH => "Witness requires only-redeemscript scriptSig".fmt(f),
			Error::WitnessUnexpected => "Witness provided for non-witness script".fmt(f),
			Error::WitnessPubKeyType => "Using non-compressed keys in segwit".fmt(f),
		}
	}
}
