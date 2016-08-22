//! Bitcoin signatures.
//!
//! http://bitcoin.stackexchange.com/q/12554/40688

use std::fmt;
use std::cmp::PartialEq;
use std::ops::Deref;
use std::str::FromStr;
use hex::{ToHex, FromHex};
use hash::H520;
use keys::Error;

#[derive(PartialEq)]
pub struct Signature(Vec<u8>);

impl fmt::Debug for Signature {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.0.to_hex().fmt(f)
	}
}

impl fmt::Display for Signature {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.0.to_hex().fmt(f)
	}
}

impl Deref for Signature {
	type Target = [u8];

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl FromStr for Signature {
	type Err = Error;

	fn from_str(s: &str) -> Result<Self, Error> {
		let vec = try!(s.from_hex().map_err(|_| Error::InvalidSignature));
		Ok(Signature(vec))
	}
}

impl From<&'static str> for Signature {
	fn from(s: &'static str) -> Self {
		s.parse().unwrap()
	}
}

impl From<Vec<u8>> for Signature {
	fn from(v: Vec<u8>) -> Self {
		Signature(v)
	}
}

impl Signature {
	pub fn check_low_s(&self) -> bool {
		unimplemented!();
	}
}

impl<'a> From<&'a [u8]> for Signature {
	fn from(v: &'a [u8]) -> Self {
		Signature(v.to_vec())
	}
}

pub struct CompactSignature(H520);

impl fmt::Debug for CompactSignature {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.0.to_hex().fmt(f)
	}
}

impl fmt::Display for CompactSignature {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.0.to_hex().fmt(f)
	}
}

impl Deref for CompactSignature {
	type Target = [u8];

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl PartialEq for CompactSignature {
	fn eq(&self, other: &Self) -> bool {
		let s_slice: &[u8] = self;
		let o_slice: &[u8] = other;
		s_slice == o_slice
	}
}

impl FromStr for CompactSignature {
	type Err = Error;

	fn from_str(s: &str) -> Result<Self, Error> {
		let vec = try!(s.from_hex().map_err(|_| Error::InvalidSignature));
		match vec.len() {
			65 => {
				let mut compact = [0u8; 65];
				compact.copy_from_slice(&vec);
				Ok(CompactSignature(compact))
			},
			_ => Err(Error::InvalidSignature)
		}
	}
}

impl From<&'static str> for CompactSignature {
	fn from(s: &'static str) -> Self {
		s.parse().unwrap()
	}
}

impl From<H520> for CompactSignature {
	fn from(h: H520) -> Self {
		CompactSignature(h)
	}
}
