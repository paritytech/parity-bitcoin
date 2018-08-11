use std::{fmt, ops};
use secp256k1::key;
use secp256k1::{Message as SecpMessage, RecoveryId, RecoverableSignature, Error as SecpError, Signature as SecpSignature};
use hex::ToHex;
use crypto::dhash160;
use hash::{H264, H520};
use {AddressHash, Error, CompactSignature, Signature, Message, SECP256K1};

/// Secret public key
pub enum Public {
	/// Normal version of public key
	Normal(H520),
	/// Compressed version of public key
	Compressed(H264),
}

impl Public {
	pub fn from_slice(data: &[u8]) -> Result<Self, Error> {
		match data.len() {
			33 => {
				let mut public = H264::default();
				public.copy_from_slice(data);
				Ok(Public::Compressed(public))
			},
			65 => {
				let mut public = H520::default();
				public.copy_from_slice(data);
				Ok(Public::Normal(public))
			},
			_ => Err(Error::InvalidPublic)
		}
	}

	pub fn address_hash(&self) -> AddressHash {
		dhash160(self)
	}

	pub fn verify(&self, message: &Message, signature: &Signature) -> Result<bool, Error> {
		let context = &SECP256K1;
		let public = try!(key::PublicKey::from_slice(context, self));
		let mut signature = try!(SecpSignature::from_der_lax(context, signature));
		signature.normalize_s(context);
		let message = try!(SecpMessage::from_slice(&**message));
		match context.verify(&message, &signature, &public) {
			Ok(_) => Ok(true),
			Err(SecpError::IncorrectSignature) => Ok(false),
			Err(x) => Err(x.into()),
		}
	}

	pub fn recover_compact(message: &Message, signature: &CompactSignature) -> Result<Self, Error> {
		let context = &SECP256K1;
		let recovery_id = (signature[0] - 27) & 3;
		let compressed = (signature[0] - 27) & 4 != 0;
		let recovery_id = try!(RecoveryId::from_i32(recovery_id as i32));
		let signature = try!(RecoverableSignature::from_compact(context, &signature[1..65], recovery_id));
		let message = try!(SecpMessage::from_slice(&**message));
		let pubkey = try!(context.recover(&message, &signature));
		let serialized = pubkey.serialize_vec(context, compressed);
		let public = if compressed {
			let mut public = H264::default();
			public.copy_from_slice(&serialized[0..33]);
			Public::Compressed(public)
		} else {
			let mut public = H520::default();
			public.copy_from_slice(&serialized[0..65]);
			Public::Normal(public)
		};
		Ok(public)
	}
}

impl ops::Deref for Public {
	type Target = [u8];

	fn deref(&self) -> &Self::Target {
		match *self {
			Public::Normal(ref hash) => &**hash,
			Public::Compressed(ref hash) => &**hash,
		}
	}
}

impl PartialEq for Public {
	fn eq(&self, other: &Self) -> bool {
		let s_slice: &[u8] = self;
		let o_slice: &[u8] = other;
		s_slice == o_slice
	}
}

impl fmt::Debug for Public {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match *self {
			Public::Normal(ref hash) => writeln!(f, "normal: {}", hash.to_hex::<String>()),
			Public::Compressed(ref hash) => writeln!(f, "compressed: {}", hash.to_hex::<String>()),
		}
	}
}

impl fmt::Display for Public {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.to_hex::<String>().fmt(f)
	}
}
