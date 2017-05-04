//! Script interpreter verification flags

/// Script interpreter verification flags
#[derive(Default, Debug, PartialEq)]
pub struct VerificationFlags {
	pub none: bool,

	/// Evaluate P2SH subscripts (softfork safe, BIP16).
	pub verify_p2sh: bool,

	/// Passing a non-strict-DER signature or one with undefined hashtype to a checksig operation causes script failure.
	/// Evaluating a pubkey that is not (0x04 + 64 bytes) or (0x02 or 0x03 + 32 bytes) by checksig causes script failure.
	/// (softfork safe, but not used or intended as a consensus rule).
	pub verify_strictenc: bool,

	/// Passing a non-strict-DER signature to a checksig operation causes script failure (softfork safe, BIP62 rule 1)
	pub verify_dersig: bool,

	/// Passing a non-strict-DER signature or one with S > order/2 to a checksig operation causes script failure
	/// (softfork safe, BIP62 rule 5).
	pub verify_low_s: bool,

	/// verify dummy stack item consumed by CHECKMULTISIG is of zero-length (softfork safe, BIP62 rule 7).
	pub verify_nulldummy: bool,

	/// Using a non-push operator in the scriptSig causes script failure (softfork safe, BIP62 rule 2).
	pub verify_sigpushonly: bool,

	/// Require minimal encodings for all push operations (OP_0... OP_16, OP_1NEGATE where possible, direct
	/// pushes up to 75 bytes, OP_PUSHDATA up to 255 bytes, OP_PUSHDATA2 for anything larger). Evaluating
	/// any other push causes the script to fail (BIP62 rule 3).
	/// In addition, whenever a stack element is interpreted as a number, it must be of minimal length (BIP62 rule 4).
	/// (softfork safe)
	pub verify_minimaldata: bool,

	/// Discourage use of NOPs reserved for upgrades (NOP1-10)
	///
	/// Provided so that nodes can avoid accepting or mining transactions
	/// containing executed NOP's whose meaning may change after a soft-fork,
	/// thus rendering the script invalid; with this flag set executing
	/// discouraged NOPs fails the script. This verification flag will never be
	/// a mandatory flag applied to scripts in a block. NOPs that are not
	/// executed, e.g.  within an unexecuted IF ENDIF block, are *not* rejected.
	pub verify_discourage_upgradable_nops: bool,

	/// Require that only a single stack element remains after evaluation. This changes the success criterion from
	/// "At least one stack element must remain, and when interpreted as a boolean, it must be true" to
	/// "Exactly one stack element must remain, and when interpreted as a boolean, it must be true".
	/// (softfork safe, BIP62 rule 6)
	/// Note: CLEANSTACK should never be used without P2SH or WITNESS.
	pub verify_cleanstack: bool,

	/// Verify CHECKLOCKTIMEVERIFY
	///
	/// See BIP65 for details.
	pub verify_locktime: bool,

	/// support CHECKSEQUENCEVERIFY opcode
	///
	/// See BIP112 for details
	pub verify_checksequence: bool,

	/// Support segregated witness
	pub verify_witness: bool,

	/// Making v1-v16 witness program non-standard
	pub verify_discourage_upgradable_witness_program: bool,
}

impl VerificationFlags {
	pub fn verify_p2sh(mut self, value: bool) -> Self {
		self.verify_p2sh = value;
		self
	}

	pub fn verify_locktime(mut self, value: bool) -> Self {
		self.verify_locktime = value;
		self
	}

	pub fn verify_checksequence(mut self, value: bool) -> Self {
		self.verify_checksequence = value;
		self
	}

	pub fn verify_dersig(mut self, value: bool) -> Self {
		self.verify_dersig = value;
		self
	}
}

