use std::fmt;
use std::ops::Deref;
use secp256k1::key;
use secp256k1::{Message as SecpMessage, RecoveryId, RecoverableSignature, Error as SecpError, Signature as SecpSignature};
use rcrypto::sha2::Sha256;
use rcrypto::ripemd160::Ripemd160;
use rcrypto::digest::Digest;
use hex::ToHex;
use hash::{H264, H520};
use keys::{AddressHash, Error, CompactSignature, Signature, Message, SECP256K1};

pub enum Public {
	Normal(H520),
	Compressed(H264),
}

impl Public {
	pub fn address_hash(&self) -> AddressHash {
		let mut tmp = [0u8; 32];
		let mut result = [0u8; 20];
		let mut sha2 = Sha256::new();
		let mut rmd = Ripemd160::new();
		sha2.input(self);
		sha2.result(&mut tmp);
		rmd.input(&tmp);
		rmd.result(&mut result);
		result
	}

	pub fn verify(&self, message: &Message, signature: &Signature) -> Result<bool, Error> {
		let context = &SECP256K1;
		let public = try!(key::PublicKey::from_slice(context, self));
		let signature = try!(SecpSignature::from_der(context, signature));
		let message = try!(SecpMessage::from_slice(message));
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
		let message = try!(SecpMessage::from_slice(message));
		let pubkey = try!(context.recover(&message, &signature));
		let serialized = pubkey.serialize_vec(context, compressed);
		let public = if compressed {
			let mut public = [0u8; 33];
			public.copy_from_slice(&serialized[0..33]);
			Public::Compressed(public)
		} else {
			let mut public = [0u8; 65];
			public.copy_from_slice(&serialized[0..65]);
			Public::Normal(public)
		};
		Ok(public)
	}
}

impl Deref for Public {
	type Target = [u8];

	fn deref(&self) -> &Self::Target {
		match *self {
			Public::Normal(ref hash) => hash,
			Public::Compressed(ref hash) => hash,
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
			Public::Normal(ref hash) => writeln!(f, "normal: {}", hash.to_hex()),
			Public::Compressed(ref hash) => writeln!(f, "compressed: {}", hash.to_hex()),
		}
	}
}

impl fmt::Display for Public {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.to_hex().fmt(f)
	}
}
