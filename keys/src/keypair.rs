//! Bitcoin key pair.

use std::fmt;
use secp256k1::key;
use hash::{H264, H520};
use network::Network;
use {Public, Error, SECP256K1, Address, Type, Private, Secret};

pub struct KeyPair {
	private: Private,
	public: Public,
}

impl fmt::Debug for KeyPair {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		try!(self.private.fmt(f));
		writeln!(f, "public: {:?}", self.public)
	}
}

impl fmt::Display for KeyPair {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		try!(writeln!(f, "private: {}", self.private));
		writeln!(f, "public: {}", self.public)
	}
}

impl KeyPair {
	pub fn private(&self) -> &Private {
		&self.private
	}

	pub fn public(&self) -> &Public {
		&self.public
	}

	pub fn from_private(private: Private) -> Result<KeyPair, Error> {
		let context = &SECP256K1;
		let s: key::SecretKey = try!(key::SecretKey::from_slice(context, &*private.secret));
		let pub_key = try!(key::PublicKey::from_secret_key(context, &s));
		let serialized = pub_key.serialize_vec(context, private.compressed);

		let public = if private.compressed {
			let mut public = H264::default();
			public.copy_from_slice(&serialized[0..33]);
			Public::Compressed(public)
		} else {
			let mut public = H520::default();
			public.copy_from_slice(&serialized[0..65]);
			Public::Normal(public)
		};

		let keypair = KeyPair {
			private: private,
			public: public,
		};

		Ok(keypair)
	}

	pub fn from_keypair(sec: key::SecretKey, public: key::PublicKey, network: Network) -> Self {
		let context = &SECP256K1;
		let serialized = public.serialize_vec(context, false);
		let mut secret = Secret::default();
		secret.copy_from_slice(&sec[0..32]);
		let mut public = H520::default();
		public.copy_from_slice(&serialized[0..65]);

		KeyPair {
			private: Private {
				network: network,
				secret: secret,
				compressed: false,
			},
			public: Public::Normal(public),
		}
	}

	pub fn address(&self) -> Address {
		Address {
			kind: Type::P2PKH,
			network: self.private.network,
			hash: self.public.address_hash(),
		}
	}
}

#[cfg(test)]
mod tests {
	use crypto::dhash256;
	use Public;
	use super::KeyPair;

	/// Tests from:
	/// https://github.com/bitcoin/bitcoin/blob/a6a860796a44a2805a58391a009ba22752f64e32/src/test/key_tests.cpp
	const SECRET_0: &'static str = "5KSCKP8NUyBZPCCQusxRwgmz9sfvJQEgbGukmmHepWw5Bzp95mu";
	const SECRET_1: &'static str = "5HxWvvfubhXpYYpS3tJkw6fq9jE9j18THftkZjHHfmFiWtmAbrj";
	const SECRET_2: &'static str = "5KC4ejrDjv152FGwP386VD1i2NYc5KkfSMyv1nGy1VGDxGHqVY3";
	const SECRET_1C: &'static str = "Kwr371tjA9u2rFSMZjTNun2PXXP3WPZu2afRHTcta6KxEUdm1vEw";
	const SECRET_2C: &'static str = "L3Hq7a8FEQwJkW1M2GNKDW28546Vp5miewcCzSqUD9kCAXrJdS3g";
	const ADDRESS_0: &'static str = "16meyfSoQV6twkAAxPe51RtMVz7PGRmWna";
	const ADDRESS_1: &'static str = "1QFqqMUD55ZV3PJEJZtaKCsQmjLT6JkjvJ";
	const ADDRESS_2: &'static str = "1F5y5E5FMc5YzdJtB9hLaUe43GDxEKXENJ";
	const ADDRESS_1C: &'static str = "1NoJrossxPBKfCHuJXT4HadJrXRE9Fxiqs";
	const ADDRESS_2C: &'static str = "1CRj2HyM1CXWzHAXLQtiGLyggNT9WQqsDs";
	const SIGN_1: &'static str = "304402205dbbddda71772d95ce91cd2d14b592cfbc1dd0aabd6a394b6c2d377bbe59d31d022014ddda21494a4e221f0824f0b8b924c43fa43c0ad57dccdaa11f81a6bd4582f6";
	const SIGN_2: &'static str = "3044022052d8a32079c11e79db95af63bb9600c5b04f21a9ca33dc129c2bfa8ac9dc1cd5022061d8ae5e0f6c1a16bde3719c64c2fd70e404b6428ab9a69566962e8771b5944d";
	const SIGN_COMPACT_1: &'static str = "1c5dbbddda71772d95ce91cd2d14b592cfbc1dd0aabd6a394b6c2d377bbe59d31d14ddda21494a4e221f0824f0b8b924c43fa43c0ad57dccdaa11f81a6bd4582f6";
	const SIGN_COMPACT_1C: &'static str = "205dbbddda71772d95ce91cd2d14b592cfbc1dd0aabd6a394b6c2d377bbe59d31d14ddda21494a4e221f0824f0b8b924c43fa43c0ad57dccdaa11f81a6bd4582f6";
	const SIGN_COMPACT_2: &'static str = "1c52d8a32079c11e79db95af63bb9600c5b04f21a9ca33dc129c2bfa8ac9dc1cd561d8ae5e0f6c1a16bde3719c64c2fd70e404b6428ab9a69566962e8771b5944d";
	const SIGN_COMPACT_2C: &'static str = "2052d8a32079c11e79db95af63bb9600c5b04f21a9ca33dc129c2bfa8ac9dc1cd561d8ae5e0f6c1a16bde3719c64c2fd70e404b6428ab9a69566962e8771b5944d";

	fn check_addresses(secret: &'static str, address: &'static str) -> bool {
		let kp = KeyPair::from_private(secret.into()).unwrap();
		kp.address() == address.into()
	}

	fn check_compressed(secret: &'static str, compressed: bool) -> bool {
		let kp = KeyPair::from_private(secret.into()).unwrap();
		kp.private().compressed == compressed
	}

	fn check_sign(secret: &'static str, raw_message: &[u8], signature: &'static str) -> bool {
		let message = dhash256(raw_message);
		let kp = KeyPair::from_private(secret.into()).unwrap();
		kp.private().sign(&message).unwrap() == signature.into()
	}

	fn check_verify(secret: &'static str, raw_message: &[u8], signature: &'static str) -> bool {
		let message = dhash256(raw_message);
		let kp = KeyPair::from_private(secret.into()).unwrap();
		kp.public().verify(&message, &signature.into()).unwrap()
	}

	fn check_sign_compact(secret: &'static str, raw_message: &[u8], signature: &'static str) -> bool {
		let message = dhash256(raw_message);
		let kp = KeyPair::from_private(secret.into()).unwrap();
		kp.private().sign_compact(&message).unwrap() == signature.into()
	}

	fn check_recover_compact(secret: &'static str, raw_message: &[u8]) -> bool {
		let message = dhash256(raw_message);
		let kp = KeyPair::from_private(secret.into()).unwrap();
		let signature = kp.private().sign_compact(&message).unwrap();
		let recovered = Public::recover_compact(&message, &signature).unwrap();
		kp.public() == &recovered
	}

	#[test]
	fn test_keypair_address() {
		assert!(check_addresses(SECRET_0, ADDRESS_0));
		assert!(check_addresses(SECRET_1, ADDRESS_1));
		assert!(check_addresses(SECRET_2, ADDRESS_2));
		assert!(check_addresses(SECRET_1C, ADDRESS_1C));
		assert!(check_addresses(SECRET_2C, ADDRESS_2C));
	}

	#[test]
	fn test_keypair_is_compressed() {
		assert!(check_compressed(SECRET_0, false));
		assert!(check_compressed(SECRET_1, false));
		assert!(check_compressed(SECRET_2, false));
		assert!(check_compressed(SECRET_1C, true));
		assert!(check_compressed(SECRET_2C, true));
	}

	#[test]
	fn test_sign() {
		let message = b"Very deterministic message";
		assert!(check_sign(SECRET_1, message, SIGN_1));
		assert!(check_sign(SECRET_1C, message, SIGN_1));
		assert!(check_sign(SECRET_2, message, SIGN_2));
		assert!(check_sign(SECRET_2C, message, SIGN_2));
		assert!(!check_sign(SECRET_2C, b"", SIGN_2));
	}

	#[test]
	fn test_verify() {
		let message = b"Very deterministic message";
		assert!(check_verify(SECRET_1, message, SIGN_1));
		assert!(check_verify(SECRET_1C, message, SIGN_1));
		assert!(check_verify(SECRET_2, message, SIGN_2));
		assert!(check_verify(SECRET_2C, message, SIGN_2));
		assert!(!check_verify(SECRET_2C, b"", SIGN_2));
	}

	#[test]
	fn test_sign_compact() {
		let message = b"Very deterministic message";
		assert!(check_sign_compact(SECRET_1, message, SIGN_COMPACT_1));
		assert!(check_sign_compact(SECRET_1C, message, SIGN_COMPACT_1C));
		assert!(check_sign_compact(SECRET_2, message, SIGN_COMPACT_2));
		assert!(check_sign_compact(SECRET_2C, message, SIGN_COMPACT_2C));
		assert!(!check_sign_compact(SECRET_2C, b"", SIGN_COMPACT_2C));
	}

	#[test]
	fn test_recover_compact() {
		let message = b"Very deterministic message";
		assert!(check_recover_compact(SECRET_0, message));
		assert!(check_recover_compact(SECRET_1, message));
		assert!(check_recover_compact(SECRET_1C, message));
		assert!(check_recover_compact(SECRET_2, message));
		assert!(check_recover_compact(SECRET_2C, message));
	}
}
