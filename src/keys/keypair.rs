//! Bitcoin KeyPair

use std::fmt;
use secp256k1::key;
use network::Network;
use keys::{Public, Error, SECP256K1, Address, Type, Private, Message};

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

	pub fn address(&self) -> Address {
		Address {
			kind: Type::P2PKH,
			network: self.private.network,
			hash: self.public.address_hash(),
		}
	}

	pub fn verify(&self, _message: &Message) -> Result<bool, Error> {
		unimplemented!();
	}

	pub fn recover_compact(&self, _signature: ()) -> Result<Public, Error> {
		unimplemented!();
	}
}

#[cfg(test)]
mod tests {
	use keys::Message;
	use crypto::hash;
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

	fn check_addresses(secret: &'static str, address: &'static str) -> bool {
		let kp = KeyPair::from_private(secret.into()).unwrap();
		kp.address() == address.into()
	}

	fn check_compressed(secret: &'static str, compressed: bool) -> bool {
		let kp = KeyPair::from_private(secret.into()).unwrap();
		kp.private().compressed == compressed
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

	// TODO: fix
	#[test]
	#[ignore]
	fn test_sign() {
		use hex::ToHex;
		let message = hash(b"Very deterministic message");
		println!("message: {}", message.to_hex());
		let kp = KeyPair::from_private(SECRET_1.into()).unwrap();
		let signature = kp.private().sign(&message).unwrap();
		assert_eq!(signature, SIGN_1.into());
	}
}
