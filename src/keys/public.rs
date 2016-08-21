use std::fmt;
use std::ops::Deref;
use rcrypto::sha2::Sha256;
use rcrypto::ripemd160::Ripemd160;
use rcrypto::digest::Digest;
use hex::ToHex;
use hash::{H264, H520};
use keys::AddressHash;

pub enum Public {
	Normal(H520),
	Compressed(H264),
}

impl Public {
	pub fn address_hash(&self) -> AddressHash {
		let mut tmp = [0u8; 32];
		let mut result = [0u8; 20];
		let mut sha2 = Sha256::new();
		let mut rmd = Ripemd160::new();
		sha2.input(self);
		sha2.result(&mut tmp);
		rmd.input(&tmp);
		rmd.result(&mut result);
		result
	}
}

impl Deref for Public {
	type Target = [u8];

	fn deref(&self) -> &Self::Target {
		match *self {
			Public::Normal(ref hash) => hash,
			Public::Compressed(ref hash) => hash,
		}
	}
}

impl fmt::Debug for Public {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match *self {
			Public::Normal(ref hash) => writeln!(f, "normal: {}", hash.to_hex()),
			Public::Compressed(ref hash) => writeln!(f, "compressed: {}", hash.to_hex()),
		}
	}
}

impl fmt::Display for Public {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.to_hex().fmt(f)
	}
}
