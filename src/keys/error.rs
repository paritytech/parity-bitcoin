use std::fmt;
use secp256k1::Error as SecpError;

#[derive(Debug, PartialEq)]
pub enum Error {
	InvalidPublic,
	InvalidSecret,
	InvalidMessage,
	InvalidSignature,
	FailedKeyGeneration,
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		let msg = match *self {
			Error::InvalidPublic => "Invalid Public",
			Error::InvalidSecret => "Invalid Secret",
			Error::InvalidMessage => "Invalid Message",
			Error::InvalidSignature => "Invalid Signature",
			Error::FailedKeyGeneration => "Key generation failed",
		};

		msg.fmt(f)
	}
}

impl From<SecpError> for Error {
	fn from(e: SecpError) -> Self {
		match e {
			SecpError::InvalidPublicKey	=> Error::InvalidPublic,
			SecpError::InvalidSecretKey => Error::InvalidSecret,
			SecpError::InvalidMessage => Error::InvalidMessage,
			_ => Error::InvalidSignature,
		}
	}
}
