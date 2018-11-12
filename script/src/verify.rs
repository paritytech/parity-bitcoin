use keys::{Public, Signature, Message};
use chain::constants::{
	SEQUENCE_FINAL, SEQUENCE_LOCKTIME_DISABLE_FLAG,
	SEQUENCE_LOCKTIME_MASK, SEQUENCE_LOCKTIME_TYPE_FLAG, LOCKTIME_THRESHOLD
};
use sign::SignatureVersion;
use {Script, TransactionInputSigner, Num};

/// Checks transaction signature
pub trait SignatureChecker {
	fn verify_signature(
		&self,
		signature: &Signature,
		public: &Public,
		hash: &Message,
	) -> bool;

	fn check_signature(
		&self,
		signature: &Signature,
		public: &Public,
		script_code: &Script,
		sighashtype: u32,
		version: SignatureVersion
	) -> bool;

	fn check_lock_time(&self, lock_time: Num) -> bool;

	fn check_sequence(&self, sequence: Num) -> bool;
}

pub struct NoopSignatureChecker;

impl SignatureChecker for NoopSignatureChecker {
	fn verify_signature(&self, signature: &Signature, public: &Public, hash: &Message) -> bool {
		public.verify(hash, signature).unwrap_or(false)
	}

	fn check_signature(&self, _: &Signature, _: &Public, _: &Script, _: u32, _: SignatureVersion) -> bool {
		false
	}

	fn check_lock_time(&self, _: Num) -> bool {
		false
	}

	fn check_sequence(&self, _: Num) -> bool {
		false
	}
}

#[derive(Debug)]
pub struct TransactionSignatureChecker {
	pub signer: TransactionInputSigner,
	pub input_index: usize,
	pub input_amount: u64,
}

impl SignatureChecker for TransactionSignatureChecker {
	fn verify_signature(
		&self,
		signature: &Signature,
		public: &Public,
		hash: &Message,
	) -> bool {
		public.verify(hash, signature).unwrap_or(false)
	}

	fn check_signature(
		&self,
		signature: &Signature,
		public: &Public,
		script_code: &Script,
		sighashtype: u32,
		version: SignatureVersion
	) -> bool {
		let hash = self.signer.signature_hash(self.input_index, self.input_amount, script_code, version, sighashtype);
		self.verify_signature(signature, public, &hash)
	}

	fn check_lock_time(&self, lock_time: Num) -> bool {
		// There are two kinds of nLockTime: lock-by-blockheight
		// and lock-by-blocktime, distinguished by whether
		// nLockTime < LOCKTIME_THRESHOLD.
		//
		// We want to compare apples to apples, so fail the script
		// unless the type of nLockTime being tested is the same as
		// the nLockTime in the transaction.
		let lock_time_u32: u32 = lock_time.into();
		if !(
			(self.signer.lock_time < LOCKTIME_THRESHOLD && lock_time_u32 < LOCKTIME_THRESHOLD) ||
			(self.signer.lock_time >= LOCKTIME_THRESHOLD && lock_time_u32 >= LOCKTIME_THRESHOLD)
		) {
			return false;
		}

		// Now that we know we're comparing apples-to-apples, the
		// comparison is a simple numeric one.
		if i64::from(lock_time) > self.signer.lock_time as i64 {
			return false;
		}

		// Finally the nLockTime feature can be disabled and thus
		// CHECKLOCKTIMEVERIFY bypassed if every txin has been
		// finalized by setting nSequence to maxint. The
		// transaction would be allowed into the blockchain, making
		// the opcode ineffective.
		//
		// Testing if this vin is not final is sufficient to
		// prevent this condition. Alternatively we could test all
		// inputs, but testing just this input minimizes the data
		// required to prove correct CHECKLOCKTIMEVERIFY execution.
		SEQUENCE_FINAL != self.signer.inputs[self.input_index].sequence
	}

	fn check_sequence(&self, sequence: Num) -> bool {
		// Relative lock times are supported by comparing the passed
		// in operand to the sequence number of the input.
		let to_sequence: i64 = self.signer.inputs[self.input_index].sequence as i64;

		// Fail if the transaction's version number is not set high
		// enough to trigger BIP 68 rules.
		if (self.signer.version as u32) < 2 {
			return false;
		}

		// Sequence numbers with their most significant bit set are not
		// consensus constrained. Testing that the transaction's sequence
		// number do not have this bit set prevents using this property
		// to get around a CHECKSEQUENCEVERIFY check.
		if to_sequence & SEQUENCE_LOCKTIME_DISABLE_FLAG as i64 != 0 {
			return false;
		}

		// Mask off any bits that do not have consensus-enforced meaning
		// before doing the integer comparisons
		let locktime_mask: u32 = SEQUENCE_LOCKTIME_TYPE_FLAG | SEQUENCE_LOCKTIME_MASK;
		let to_sequence_masked: i64 = to_sequence & locktime_mask as i64;
		let sequence_masked: i64 = i64::from(sequence) & locktime_mask as i64;

		// There are two kinds of nSequence: lock-by-blockheight
		// and lock-by-blocktime, distinguished by whether
		// nSequenceMasked < CTxIn::SEQUENCE_LOCKTIME_TYPE_FLAG.

		// We want to compare apples to apples, so fail the script
		// unless the type of nSequenceMasked being tested is the same as
		// the nSequenceMasked in the transaction.
		if !(
			(to_sequence_masked < SEQUENCE_LOCKTIME_TYPE_FLAG as i64 && sequence_masked < SEQUENCE_LOCKTIME_TYPE_FLAG as i64) ||
			(to_sequence_masked >= SEQUENCE_LOCKTIME_TYPE_FLAG as i64 && sequence_masked >= SEQUENCE_LOCKTIME_TYPE_FLAG as i64)
		) {
			return false;
		}

		// Now that we know we're comparing apples-to-apples, the
		// comparison is a simple numeric one.
		sequence_masked <= to_sequence_masked
	}
}
