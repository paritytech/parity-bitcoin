//! Bitcoin signatures.
//!
//! http://bitcoin.stackexchange.com/q/12554/40688

use std::{fmt, ops, str};
use hex::{ToHex, FromHex};
use hash::H520;
use Error;

#[derive(PartialEq)]
pub struct Signature(Vec<u8>);

impl fmt::Debug for Signature {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.0.to_hex::<String>().fmt(f)
	}
}

impl fmt::Display for Signature {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.0.to_hex::<String>().fmt(f)
	}
}

impl ops::Deref for Signature {
	type Target = [u8];

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl str::FromStr for Signature {
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

impl From<Signature> for Vec<u8> {
	fn from(s: Signature) -> Self {
		s.0
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

#[derive(PartialEq)]
pub struct CompactSignature(H520);

impl fmt::Debug for CompactSignature {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.write_str(&self.0.to_hex::<String>())
	}
}

impl fmt::Display for CompactSignature {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.write_str(&self.0.to_hex::<String>())
	}
}

impl ops::Deref for CompactSignature {
	type Target = [u8];

	fn deref(&self) -> &Self::Target {
		&*self.0
	}
}

impl str::FromStr for CompactSignature {
	type Err = Error;

	fn from_str(s: &str) -> Result<Self, Error> {
		match s.parse() {
			Ok(hash) => Ok(CompactSignature(hash)),
			_ => Err(Error::InvalidSignature),
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
