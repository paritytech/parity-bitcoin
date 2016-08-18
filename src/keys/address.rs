//! A Bitcoin address, or simply address, is an identifier of 26-35 alphanumeric characters, beginning with the number 1
//! or 3, that represents a possible destination for a bitcoin payment.
//!
//! https://en.bitcoin.it/wiki/Address

use std::fmt;
use std::ops::Deref;
use base58::ToBase58;
use network::Network;
use hash::H160;
use keys::{DisplayLayout, checksum, Error};

/// There are two address formats currently in use.
/// https://bitcoin.org/en/developer-reference#address-conversion
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Type {
	/// Pay to PubKey Hash
	/// Common P2PKH which begin with the number 1, eg: 1BvBMSEYstWetqTFn5Au4m4GFg7xJaNVN2.
	/// https://bitcoin.org/en/glossary/p2pkh-address
	P2PKH,
	/// Pay to Script Hash
	/// Newer P2SH type starting with the number 3, eg: 3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy.
	/// https://bitcoin.org/en/glossary/p2sh-address
	P2SH,
}

#[derive(Debug, PartialEq)]
pub struct Address {
	/// The type of the address.
	pub kind: Type,
	/// The network of the address.
	pub network: Network,
	/// Public key hash.
	pub hash: H160,
}

pub struct AddressDisplayLayout([u8; 25]);

impl Deref for AddressDisplayLayout {
	type Target = [u8];

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl DisplayLayout for Address {
	type Target = AddressDisplayLayout;

	fn layout(&self) -> Self::Target {
		let mut result = [0u8; 25];

		result[0] = match (self.network, self.kind) {
			(Network::Mainnet, Type::P2PKH) => 0,
			(Network::Mainnet, Type::P2SH) => 5,
			(Network::Testnet, Type::P2PKH) => 111,
			(Network::Testnet, Type::P2SH) => 196,
		};

		result[1..21].copy_from_slice(&self.hash);
		let cs = checksum(&result[0..21]);
		result[21..25].copy_from_slice(&cs);
		AddressDisplayLayout(result)
	}

	fn from_layout(_data: &[u8]) -> Result<Self, Error> where Self: Sized {
		unimplemented!();
	}
}

impl fmt::Display for Address {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.layout().to_base58().fmt(f)
	}
}
