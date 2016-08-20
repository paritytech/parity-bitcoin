//! Bitcoin KeyPair

use std::fmt;
use rcrypto::sha2::Sha256;
use rcrypto::ripemd160::Ripemd160;
use rcrypto::digest::Digest;
use secp256k1::key;
use network::Network;
use keys::{Public, Error, SECP256K1, Address, Type, AddressHash, Private, Message};

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
	pub fn from_private(private: Private) -> Result<KeyPair, Error> {
		let context = &SECP256K1;
		let s: key::SecretKey = try!(key::SecretKey::from_slice(context, &private.secret));
		let pub_key = try!(key::PublicKey::from_secret_key(context, &s));
		let serialized = pub_key.serialize_vec(context, private.compressed);

		let public = if private.compressed {
			let mut public = [0u8; 33];
			public.copy_from_slice(&serialized[0..33]);
			Public::Compressed(public)
		} else {
			let mut public = [0u8; 65];
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
		let mut secret = [0u8; 32];
		secret.copy_from_slice(&sec[0..32]);
		let mut public = [0u8; 65];
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

	pub fn address_hash(&self) -> AddressHash {
		let mut tmp = [0u8; 32];
		let mut result = [0u8; 20];
		let mut sha2 = Sha256::new();
		let mut rmd = Ripemd160::new();
		sha2.input(&self.public);
		sha2.result(&mut tmp);
		rmd.input(&tmp);
		rmd.result(&mut result);
		result
	}

	pub fn address(&self) -> Address {
		Address {
			kind: Type::P2PKH,
			network: self.private.network,
			hash: self.address_hash(),
		}
	}

	pub fn is_compressed(&self) -> bool {
		self.private.compressed
	}

	pub fn sign(&self, _message: &Message) -> Result<(), Error> {
		unimplemented!();
	}

	pub fn sign_compact(&self, _message: &Message) -> Result<(), Error> {
		unimplemented!();
	}

	pub fn verify(&self, _message: &Message) -> Result<bool, Error> {
		unimplemented!();
	}

	pub fn recover_compat(&self, _signature: ()) -> Result<Public, Error> {
		unimplemented!();
	}
}

#[cfg(test)]
mod tests {
	use super::KeyPair;

	/// Tests from:
	/// https://github.com/bitcoin/bitcoin/blob/a6a860796a44a2805a58391a009ba22752f64e32/src/test/key_tests.cpp
	const SECRET_0: &'static str = "5KSCKP8NUyBZPCCQusxRwgmz9sfvJQEgbGukmmHepWw5Bzp95mu";
	const SECRET_1: &'static str = "5HxWvvfubhXpYYpS3tJkw6fq9jE9j18THftkZjHHfmFiWtmAbrj";
	const SECRET_2: &'static str = "5KC4ejrDjv152FGwP386VD1i2NYc5KkfSMyv1nGy1VGDxGHqVY3";
	const SECRET_3: &'static str = "Kwr371tjA9u2rFSMZjTNun2PXXP3WPZu2afRHTcta6KxEUdm1vEw";
	const SECRET_4: &'static str = "L3Hq7a8FEQwJkW1M2GNKDW28546Vp5miewcCzSqUD9kCAXrJdS3g";
	const ADDRESS_0: &'static str = "16meyfSoQV6twkAAxPe51RtMVz7PGRmWna";
	const ADDRESS_1: &'static str = "1QFqqMUD55ZV3PJEJZtaKCsQmjLT6JkjvJ";
	const ADDRESS_2: &'static str = "1F5y5E5FMc5YzdJtB9hLaUe43GDxEKXENJ";
	const ADDRESS_3: &'static str = "1NoJrossxPBKfCHuJXT4HadJrXRE9Fxiqs";
	const ADDRESS_4: &'static str = "1CRj2HyM1CXWzHAXLQtiGLyggNT9WQqsDs";

	fn check_addresses(secret: &'static str, address: &'static str) -> bool {
		let kp = KeyPair::from_private(secret.into()).unwrap();
		kp.address() == address.into()
	}

	fn check_compressed(secret: &'static str, compressed: bool) -> bool {
		let kp = KeyPair::from_private(secret.into()).unwrap();
		kp.is_compressed() == compressed
	}

	#[test]
	fn test_keypair_address() {
		assert!(check_addresses(SECRET_0, ADDRESS_0));
		assert!(check_addresses(SECRET_1, ADDRESS_1));
		assert!(check_addresses(SECRET_2, ADDRESS_2));
		assert!(check_addresses(SECRET_3, ADDRESS_3));
		assert!(check_addresses(SECRET_4, ADDRESS_4));
	}

	#[test]
	fn test_keypair_is_compressed() {
		assert!(check_compressed(SECRET_0, false));
		assert!(check_compressed(SECRET_1, false));
		assert!(check_compressed(SECRET_2, false));
		assert!(check_compressed(SECRET_3, true));
		assert!(check_compressed(SECRET_4, true));
	}
}
