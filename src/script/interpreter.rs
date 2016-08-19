use keys::Public;
use super::{Script, Num, VerificationFlags};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum SignatureVersion {
	Base,
	Witness_V0,
}

pub trait SignatureChecker {
	fn check_signature(&self, script_signature: &[u8], public: &Public, script: &Script, version: SignatureVersion);

	fn check_lock_time(&self, lock_time: Num);

	fn check_sequence(&self, sequence: Num);
}

pub fn eval_script(stack: &mut Vec<Vec<u8>>, script: &Script, flags: &VerificationFlags, checker: &SignatureChecker, version: SignatureVersion) -> Result<bool, ()> {
	Ok(false)
}
