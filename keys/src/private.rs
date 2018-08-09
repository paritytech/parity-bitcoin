//! Secret with additional network identifier and format type

use std::fmt;
use std::str::FromStr;
use secp256k1::key;
use secp256k1::Message as SecpMessage;
use hex::ToHex;
use base58::{ToBase58, FromBase58};
use crypto::checksum;
use hash::H520;
use network::Network;
use {Secret, DisplayLayout, Error, Message, Signature, CompactSignature, SECP256K1};

/// Secret with additional network identifier and format type
#[derive(PartialEq)]
pub struct Private {
	/// The network on which this key should be used.
	pub network: Network,
	/// ECDSA key.
	pub secret: Secret,
	/// True if this private key represents a compressed address.
	pub compressed: bool,
}

impl Private {
	pub fn sign(&self, message: &Message) -> Result<Signature, Error> {
		let context = &SECP256K1;
		let secret = try!(key::SecretKey::from_slice(context, &*self.secret));
		let message = try!(SecpMessage::from_slice(&**message));
		let signature = try!(context.sign(&message, &secret));
		let data = signature.serialize_der(context);
		Ok(data.into())
	}

	pub fn sign_compact(&self, message: &Message) -> Result<CompactSignature, Error> {
		let context = &SECP256K1;
		let secret = try!(key::SecretKey::from_slice(context, &*self.secret));
		let message = try!(SecpMessage::from_slice(&**message));
		let signature = try!(context.sign_recoverable(&message, &secret));
		let (recovery_id, data) = signature.serialize_compact(context);
		let recovery_id = recovery_id.to_i32() as u8;
		let mut signature = H520::default();
		signature[1..65].copy_from_slice(&data[0..64]);
		if self.compressed {
			signature[0] = 27 + recovery_id + 4;
		} else {
			signature[0] = 27 + recovery_id;
		}
		Ok(signature.into())
	}
}

impl DisplayLayout for Private {
	type Target = Vec<u8>;

	fn layout(&self) -> Self::Target {
		let mut result = vec![];
		let network_byte = match self.network {
			Network::Mainnet => 128,
			Network::Testnet => 239,
		};

		result.push(network_byte);
		result.extend(&*self.secret);
		if self.compressed {
			result.push(1);
		}
		let cs = checksum(&result);
		result.extend_from_slice(&*cs);
		result
	}

	fn from_layout(data: &[u8]) -> Result<Self, Error> where Self: Sized {
		let compressed = match data.len() {
			37 => false,
			38 => true,
			_ => return Err(Error::InvalidPrivate),
		};

		if compressed && data[data.len() - 5] != 1 {
			return Err(Error::InvalidPrivate);
		}

		let cs = checksum(&data[0..data.len() - 4]);
		if &data[data.len() - 4..] != &*cs {
			return Err(Error::InvalidChecksum);
		}

		let network = match data[0] {
			128 => Network::Mainnet,
			239 => Network::Testnet,
			_ => return Err(Error::InvalidPrivate),
		};

		let mut secret = Secret::default();
		secret.copy_from_slice(&data[1..33]);

		let private = Private {
			network: network,
			secret: secret,
			compressed: compressed,
		};

		Ok(private)
	}
}

impl fmt::Debug for Private {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		try!(writeln!(f, "network: {:?}", self.network));
		try!(writeln!(f, "secret: {}", self.secret.to_hex::<String>()));
		writeln!(f, "compressed: {}", self.compressed)
	}
}

impl fmt::Display for Private {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.layout().to_base58().fmt(f)
	}
}

impl FromStr for Private {
	type Err = Error;

	fn from_str(s: &str) -> Result<Self, Error> where Self: Sized {
		let hex = try!(s.from_base58().map_err(|_| Error::InvalidPrivate));
		Private::from_layout(&hex)
	}
}

impl From<&'static str> for Private {
	fn from(s: &'static str) -> Self {
		s.parse().unwrap()
	}
}

#[cfg(test)]
mod tests {
	use hash::H256;
	use network::Network;
	use super::Private;

	#[test]
	fn test_private_to_string() {
		let private = Private {
			network: Network::Mainnet,
			secret: H256::from_reversed_str("063377054c25f98bc538ac8dd2cf9064dd5d253a725ece0628a34e2f84803bd5"),
			compressed: false,
		};

		assert_eq!("5KSCKP8NUyBZPCCQusxRwgmz9sfvJQEgbGukmmHepWw5Bzp95mu".to_owned(), private.to_string());
	}

	#[test]
	fn test_private_from_str() {
		let private = Private {
			network: Network::Mainnet,
			secret: H256::from_reversed_str("063377054c25f98bc538ac8dd2cf9064dd5d253a725ece0628a34e2f84803bd5"),
			compressed: false,
		};

		assert_eq!(private, "5KSCKP8NUyBZPCCQusxRwgmz9sfvJQEgbGukmmHepWw5Bzp95mu".into());
	}
}
