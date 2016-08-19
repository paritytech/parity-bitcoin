//! Bitcoin KeyPair

use std::fmt;
use hex::ToHex;
use rcrypto::sha2::Sha256;
use rcrypto::ripemd160::Ripemd160;
use rcrypto::digest::Digest;
use secp256k1::key;
use network::Network;
use keys::{Public, Error, SECP256K1, Address, Type, AddressHash, Private};

pub struct KeyPair {
	private: Private,
	/// Uncompressed public key. 65 bytes
	/// TODO: make it optionally compressed
	public: Public,
}

impl fmt::Display for KeyPair {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		try!(writeln!(f, "private: {}", self.private.to_string()));
		writeln!(f, "public: {}", self.public.to_hex())
	}
}

impl KeyPair {
	pub fn from_private(private: Private) -> Result<KeyPair, Error> {
		let context = &SECP256K1;
		let s: key::SecretKey = try!(key::SecretKey::from_slice(context, &private.secret));
		let pub_key = try!(key::PublicKey::from_secret_key(context, &s));
		// TODO: take into account private field `compressed`
		let serialized = pub_key.serialize_vec(context, false);

		let mut public = [0u8; 65];
		public.copy_from_slice(&serialized[0..65]);

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
			public: public,
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
}

#[cfg(test)]
mod tests {
	use super::KeyPair;

	#[test]
	fn test_generating_address() {
		// secret: 5KSCKP8NUyBZPCCQusxRwgmz9sfvJQEgbGukmmHepWw5Bzp95mu
		// address: 16meyfSoQV6twkAAxPe51RtMVz7PGRmWna
		let kp = KeyPair::from_private("5KSCKP8NUyBZPCCQusxRwgmz9sfvJQEgbGukmmHepWw5Bzp95mu".into()).unwrap();
		assert_eq!(kp.address(), "16meyfSoQV6twkAAxPe51RtMVz7PGRmWna".into());
	}
}
