use std::fmt;
use std::cmp::PartialEq;
use std::ops::Deref;
use std::str::FromStr;
use hex::{ToHex, FromHex};
use hash::H520;
use keys::Error;

/// http://bitcoin.stackexchange.com/q/12554/40688
pub enum Signature {
	DER(Vec<u8>),
	Compact(H520),
}

impl Deref for Signature {
	type Target = [u8];

	fn deref(&self) -> &Self::Target {
		match *self {
			Signature::DER(ref hash) => hash,
			Signature::Compact(ref hash) => hash,
		}
	}
}

impl PartialEq for Signature {
	fn eq(&self, other: &Signature) -> bool {
		let s_slice: &[u8] = self;
		let o_slice: &[u8] = other;
		s_slice == o_slice
	}
}

impl fmt::Debug for Signature {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match *self {
			Signature::DER(ref hash) => writeln!(f, "normal: {}", hash.to_hex()),
			Signature::Compact(ref hash) => writeln!(f, "compact: {}", hash.to_hex()),
		}
	}
}

impl fmt::Display for Signature {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.to_hex().fmt(f)
	}
}

impl FromStr for Signature {
	type Err = Error;

	fn from_str(s: &str) -> Result<Self, Error> {
		let vec = try!(s.from_hex().map_err(|_| Error::InvalidSignature));
		let signature = match vec.len() {
			65 => {
				let mut compact = [0u8; 65];
				compact.copy_from_slice(&vec);
				Signature::Compact(compact)
			},
			_ => Signature::DER(vec),
		};
		Ok(signature)
	}
}

impl From<&'static str> for Signature {
	fn from(s: &'static str) -> Self {
		s.parse().unwrap()
	}
}
