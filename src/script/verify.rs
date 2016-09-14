use keys::{Public, Signature};
use script::{SignatureVersion, Script, TransactionInputSigner, Num};


pub trait SignatureChecker {
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

pub struct TransactionSignatureChecker {
	pub signer: TransactionInputSigner,
	pub input_index: usize,
}

impl SignatureChecker for TransactionSignatureChecker {
	fn check_signature(
		&self,
		signature: &Signature,
		public: &Public,
		script_code: &Script,
		sighashtype: u32,
		_version: SignatureVersion
	) -> bool {
		let hash = self.signer.signature_hash(self.input_index, script_code, sighashtype);
		public.verify(&hash, signature).unwrap_or(false)
	}

	fn check_lock_time(&self, _lock_time: Num) -> bool {
		unimplemented!();
	}

	fn check_sequence(&self, _sequence: Num) -> bool {
		unimplemented!();
	}
}
