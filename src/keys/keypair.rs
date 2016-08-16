use std::fmt;
use rustc_serialize::hex::ToHex;
use secp256k1::key;
use keys::{Secret, Public, Error};

pub struct KeyPair {
	secret: Secret,
	public: Public,
}

impl fmt::Display for KeyPair {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		try!(writeln!(f, "secret: {}", self.secret.to_hex()));
		writeln!(f, "public: {}", self.public.to_hex())
	}
}

impl KeyPair {
	pub fn from_secret(_secret: Secret) -> Result<KeyPair, Error> {
		unimplemented!();
	}

	pub fn from_keypair(_secret: key::SecretKey, _public: key::PublicKey) -> Result<KeyPair, Error> {
		unimplemented!();
	}
}
