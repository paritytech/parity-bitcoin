use std::fmt;
use std::ops::Deref;
use hex::ToHex;
use hash::{H264, H520};

pub enum Public {
	Normal(H520),
	Compressed(H264),
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
			Public::Compressed(ref hash) => writeln!(f, "normal: {}", hash.to_hex()),
		}
	}
}

impl fmt::Display for Public {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.to_hex().fmt(f)
	}
}
