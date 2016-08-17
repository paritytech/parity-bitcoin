use std::fmt;
use rustc_serialize::hex::ToHex;
use rcrypto::sha2::Sha256;
use rcrypto::ripemd160::Ripemd160;
use rcrypto::digest::Digest;
use secp256k1::key;
use keys::{Secret, Public, Error, SECP256K1, Address};

pub struct KeyPair {
	secret: Secret,
	/// Uncompressed public key. 65 bytes
	public: Public,
}

impl fmt::Display for KeyPair {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		try!(writeln!(f, "secret: {}", self.secret.to_hex()));
		writeln!(f, "public: {}", self.public.to_hex())
	}
}

impl KeyPair {
	pub fn from_secret(secret: Secret) -> Result<KeyPair, Error> {
		let context = &SECP256K1;
		let s: key::SecretKey = try!(key::SecretKey::from_slice(context, &secret[..]));
		let pub_key = try!(key::PublicKey::from_secret_key(context, &s));
		let serialized = pub_key.serialize_vec(context, false);

		let mut public = [0u8; 65];
		public.copy_from_slice(&serialized[0..65]);

		let keypair = KeyPair {
			secret: secret,
			public: public,
		};

		Ok(keypair)
	}

	pub fn from_keypair(sec: key::SecretKey, public: key::PublicKey) -> Self {
		let context = &SECP256K1;
		let serialized = public.serialize_vec(context, false);
		let mut secret = [0u8; 32];
		secret.copy_from_slice(&sec[0..32]);
		let mut public = [0u8; 65];
		public.copy_from_slice(&serialized[0..65]);

		KeyPair {
			secret: secret,
			public: public,
		}
	}

	pub fn address(&self) -> Address {
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
}

#[cfg(test)]
mod tests {
	use super::KeyPair;
	use base58::{ToBase58, FromBase58};

	#[test]
	fn test_generating_address() {
		// secret: 5KSCKP8NUyBZPCCQusxRwgmz9sfvJQEgbGukmmHepWw5Bzp95mu
		// address: 16meyfSoQV6twkAAxPe51RtMVz7PGRmWna
		let mut secret = [0u8; 32];
		secret.copy_from_slice(&"5KSCKP8NUyBZPCCQusxRwgmz9sfvJQEgbGukmmHepWw5Bzp95mu".from_base58().unwrap()[1..33]);
		let mut address = [0u8; 20];
		address.copy_from_slice(&"16meyfSoQV6twkAAxPe51RtMVz7PGRmWna".from_base58().unwrap()[1..21]);
		let kp = KeyPair::from_secret(secret).unwrap();
		assert_eq!(kp.address(), address);
	}
}
