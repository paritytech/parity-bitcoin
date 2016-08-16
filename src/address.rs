//! A Bitcoin address, or simply address, is an identifier of 26-35 alphanumeric characters, beginning with the number 1
//! or 3, that represents a possible destination for a bitcoin payment.
//!
//! https://en.bitcoin.it/wiki/Address

/// There are two address formats currently in use.
/// https://bitcoin.org/en/developer-reference#address-conversion
pub enum Format {
	/// Pay to PubKey Hash
	/// Common P2PKH which begin with the number 1, eg: 1BvBMSEYstWetqTFn5Au4m4GFg7xJaNVN2.
	/// https://bitcoin.org/en/glossary/p2pkh-address
	P2PKH,
	/// Pay to Script Hash
	/// Newer P2SH type starting with the number 3, eg: 3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy.
	/// https://bitcoin.org/en/glossary/p2sh-address
	P2SH,
}


