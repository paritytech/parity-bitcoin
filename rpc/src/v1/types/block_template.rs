// TODO: remove after implementing getblocktmplate RPC
#![warn(dead_code)]

use std::collections::HashMap;
use super::hash::H256;
use super::raw_transaction::RawTransaction;

/// Block template as described in:
/// https://github.com/bitcoin/bips/blob/master/bip-0022.mediawiki
/// https://github.com/bitcoin/bips/blob/master/bip-0023.mediawiki
/// https://github.com/bitcoin/bips/blob/master/bip-0009.mediawiki#getblocktemplate_changes
/// https://github.com/bitcoin/bips/blob/master/bip-0145.mediawiki
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct BlockTemplate {
	/// The preferred block version
	pub version: u32,
	/// Specific block rules that are to be enforced
	pub rules: Vec<String>,
	/// Set of pending, supported versionbit (BIP 9) softfork deployments
	/// Keys: named softfork rules
	/// Values: identifies the bit number as indicating acceptance and readiness for given key
	pub vbavailable: Option<HashMap<String, u32>>,
	/// Bit mask of versionbits the server requires set in submissions
	pub vbrequired: Option<u32>,
	/// The hash of previous (best known) block
	pub previousblockhash: H256,
	/// Contents of non-coinbase transactions that should be included in the next block
	pub transactions: Vec<BlockTemplateTransaction>,
	/// Data that should be included in the coinbase's scriptSig content
	/// Keys: ignored
	/// Values: value to be included in scriptSig
	pub coinbaseaux: Option<HashMap<String, String>>,
	/// Maximum allowable input to coinbase transaction, including the generation award and transaction fees (in Satoshis)
	pub coinbasevalue: Option<u64>,
	/// information for coinbase transaction
	pub coinbasetxn: Option<BlockTemplateTransaction>,
	/// The hash target
	pub target: H256,
	/// The minimum timestamp appropriate for next block time in seconds since epoch (Jan 1 1970 GMT)
	pub mintime: Option<i64>,
	/// List of ways the block template may be changed, e.g. 'time', 'transactions', 'prevblock'
	pub mutable: Option<Vec<String>>,
	/// A range of valid nonces (constant 00000000ffffffff)
	pub noncerange: Option<String>,
	/// Limit of sigops in blocks
	pub sigoplimit: Option<i64>,
	/// Limit of block size
	pub sizelimit: Option<u32>,
	/// Limit of block weight
	pub weightlimit: Option<u32>,
	/// Current timestamp in seconds since epoch (Jan 1 1970 GMT)
	pub curtime: i64,
	/// Compressed target of next block
	pub bits: u32,
	/// The height of the next block
	pub height: u32,
}

/// Transaction data as included in `BlockTemplate`
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct BlockTemplateTransaction {
	/// Transaction data encoded in hexadecimal
	pub data: RawTransaction,
	/// Transaction id encoded in little-endian hexadecimal
	pub txid: Option<H256>,
	/// Hash encoded in little-endian hexadecimal (including witness data)
	pub hash: Option<H256>,
	/// Transactions before this one (by 1-based index in 'transactions' list) that must be present in the final block if this one is
	pub depends: Option<Vec<u64>>,
	/// Difference in value between transaction inputs and outputs (in Satoshis).
	/// For coinbase transactions, this is a negative Number of the total collected block fees (ie, not including the block subsidy).
	/// If key is not present, fee is unknown and clients MUST NOT assume there isn't one
	pub fee: Option<i64>,
	/// Total SigOps cost, as counted for purposes of block limits.
	/// If key is not present, sigop cost is unknown and clients MUST NOT assume it is zero.
	pub sigops: Option<i64>,
	/// Total transaction weight, as counted for purposes of block limits.
	pub weight: Option<i64>,
	/// If provided and true, this transaction must be in the final block
	pub required: bool,
}

#[cfg(test)]
mod tests {
	use serde_json;
	use super::super::hash::H256;
	use super::super::bytes::Bytes;
	use rustc_serialize::hex::FromHex;
	use super::*;

	#[test]
	fn block_template_transaction_serialize() {
		assert_eq!(serde_json::to_string(&BlockTemplateTransaction {
			data: Bytes("00010203".from_hex().unwrap()),
			txid: None,
			hash: None,
			depends: None,
			fee: None,
			sigops: None,
			weight: None,
			required: false,
		}).unwrap(), r#"{"data":"00010203","txid":null,"hash":null,"depends":null,"fee":null,"sigops":null,"weight":null,"required":false}"#);
		assert_eq!(serde_json::to_string(&BlockTemplateTransaction {
			data: Bytes("00010203".from_hex().unwrap()),
			txid: Some(H256::from(1)),
			hash: Some(H256::from(2)),
			depends: Some(vec![1, 2]),
			fee: Some(100),
			sigops: Some(200),
			weight: Some(300),
			required: true,
		}).unwrap(), r#"{"data":"00010203","txid":"0100000000000000000000000000000000000000000000000000000000000000","hash":"0200000000000000000000000000000000000000000000000000000000000000","depends":[1,2],"fee":100,"sigops":200,"weight":300,"required":true}"#);
	}

	#[test]
	fn block_template_transaction_deserialize() {
		assert_eq!(
			serde_json::from_str::<BlockTemplateTransaction>(r#"{"data":"00010203","txid":null,"hash":null,"depends":null,"fee":null,"sigops":null,"weight":null,"required":false}"#).unwrap(),
			BlockTemplateTransaction {
				data: Bytes("00010203".from_hex().unwrap()),
				txid: None,
				hash: None,
				depends: None,
				fee: None,
				sigops: None,
				weight: None,
				required: false,
			});
		assert_eq!(
			serde_json::from_str::<BlockTemplateTransaction>(r#"{"data":"00010203","txid":"0100000000000000000000000000000000000000000000000000000000000000","hash":"0200000000000000000000000000000000000000000000000000000000000000","depends":[1,2],"fee":100,"sigops":200,"weight":300,"required":true}"#).unwrap(),
			BlockTemplateTransaction {
				data: Bytes("00010203".from_hex().unwrap()),
				txid: Some(H256::from(1)),
				hash: Some(H256::from(2)),
				depends: Some(vec![1, 2]),
				fee: Some(100),
				sigops: Some(200),
				weight: Some(300),
				required: true,
			});
	}
}
