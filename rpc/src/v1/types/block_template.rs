use std::collections::HashMap;
use super::hash::H256;
use chain;
use super::transaction::RawTransaction;
use miner;

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
	pub rules: Option<Vec<String>>,
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
	pub sigoplimit: Option<u32>,
	/// Limit of block size
	pub sizelimit: Option<u32>,
	/// Limit of block weight
	pub weightlimit: Option<u32>,
	/// Current timestamp in seconds since epoch (Jan 1 1970 GMT)
	pub curtime: u32,
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

impl From<miner::BlockTemplate> for BlockTemplate {
	fn from(block: miner::BlockTemplate) -> Self {
		BlockTemplate {
			version: block.version,
			previousblockhash: block.previous_header_hash.reversed().into(),
			curtime: block.time,
			bits: block.bits.into(),
			height: block.height,
			transactions: block.transactions.into_iter().map(Into::into).collect(),
			coinbasevalue: Some(block.coinbase_value),
			sizelimit: Some(block.size_limit),
			sigoplimit: Some(block.sigop_limit),
			..Default::default()
		}
	}
}

impl From<chain::IndexedTransaction> for BlockTemplateTransaction {
	fn from(transaction: chain::IndexedTransaction) -> Self {
		use ser::serialize;
		let serialize = serialize(&transaction.raw);
		BlockTemplateTransaction {
			data: RawTransaction::new(Vec::from((*serialize).clone())),
			..Default::default()
		}
	}
}


#[cfg(test)]
mod tests {
	use serde_json;
	use super::super::hash::H256;
	use super::super::bytes::Bytes;
	use hex::FromHex;
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

	#[test]
	fn block_template_serialize() {
		assert_eq!(serde_json::to_string(&BlockTemplate {
			version: 0,
			rules: None,
			vbavailable: None,
			vbrequired: None,
			previousblockhash: H256::default(),
			transactions: vec![],
			coinbaseaux: None,
			coinbasevalue: None,
			coinbasetxn: None,
			target: H256::default(),
			mintime: None,
			mutable: None,
			noncerange: None,
			sigoplimit: None,
			sizelimit: None,
			weightlimit: None,
			curtime: 100,
			bits: 200,
			height: 300,
		}).unwrap(), r#"{"version":0,"rules":null,"vbavailable":null,"vbrequired":null,"previousblockhash":"0000000000000000000000000000000000000000000000000000000000000000","transactions":[],"coinbaseaux":null,"coinbasevalue":null,"coinbasetxn":null,"target":"0000000000000000000000000000000000000000000000000000000000000000","mintime":null,"mutable":null,"noncerange":null,"sigoplimit":null,"sizelimit":null,"weightlimit":null,"curtime":100,"bits":200,"height":300}"#);
		assert_eq!(serde_json::to_string(&BlockTemplate {
			version: 0,
			rules: Some(vec!["a".to_owned()]),
			vbavailable: Some(vec![("b".to_owned(), 5)].into_iter().collect()),
			vbrequired: Some(10),
			previousblockhash: H256::from(10),
			transactions: vec![BlockTemplateTransaction {
				data: Bytes("00010203".from_hex().unwrap()),
				txid: None,
				hash: None,
				depends: None,
				fee: None,
				sigops: None,
				weight: None,
				required: false,
			}],
			coinbaseaux: Some(vec![("c".to_owned(), "d".to_owned())].into_iter().collect()),
			coinbasevalue: Some(30),
			coinbasetxn: Some(BlockTemplateTransaction {
				data: Bytes("555555".from_hex().unwrap()),
				txid: Some(H256::from(44)),
				hash: Some(H256::from(55)),
				depends: Some(vec![1]),
				fee: Some(300),
				sigops: Some(400),
				weight: Some(500),
				required: true,
			}),
			target: H256::from(100),
			mintime: Some(7),
			mutable: Some(vec!["afg".to_owned()]),
			noncerange: Some("00000000ffffffff".to_owned()),
			sigoplimit: Some(45),
			sizelimit: Some(449),
			weightlimit: Some(523),
			curtime: 100,
			bits: 200,
			height: 300,
		}).unwrap(), r#"{"version":0,"rules":["a"],"vbavailable":{"b":5},"vbrequired":10,"previousblockhash":"0a00000000000000000000000000000000000000000000000000000000000000","transactions":[{"data":"00010203","txid":null,"hash":null,"depends":null,"fee":null,"sigops":null,"weight":null,"required":false}],"coinbaseaux":{"c":"d"},"coinbasevalue":30,"coinbasetxn":{"data":"555555","txid":"2c00000000000000000000000000000000000000000000000000000000000000","hash":"3700000000000000000000000000000000000000000000000000000000000000","depends":[1],"fee":300,"sigops":400,"weight":500,"required":true},"target":"6400000000000000000000000000000000000000000000000000000000000000","mintime":7,"mutable":["afg"],"noncerange":"00000000ffffffff","sigoplimit":45,"sizelimit":449,"weightlimit":523,"curtime":100,"bits":200,"height":300}"#);
	}

	#[test]
	fn block_template_deserialize() {
		assert_eq!(
			serde_json::from_str::<BlockTemplate>(r#"{"version":0,"rules":null,"vbavailable":null,"vbrequired":null,"previousblockhash":"0000000000000000000000000000000000000000000000000000000000000000","transactions":[],"coinbaseaux":null,"coinbasevalue":null,"coinbasetxn":null,"target":"0000000000000000000000000000000000000000000000000000000000000000","mintime":null,"mutable":null,"noncerange":null,"sigoplimit":null,"sizelimit":null,"weightlimit":null,"curtime":100,"bits":200,"height":300}"#).unwrap(),
			BlockTemplate {
				version: 0,
				rules: None,
				vbavailable: None,
				vbrequired: None,
				previousblockhash: H256::default(),
				transactions: vec![],
				coinbaseaux: None,
				coinbasevalue: None,
				coinbasetxn: None,
				target: H256::default(),
				mintime: None,
				mutable: None,
				noncerange: None,
				sigoplimit: None,
				sizelimit: None,
				weightlimit: None,
				curtime: 100,
				bits: 200,
				height: 300,
			});
		assert_eq!(
			serde_json::from_str::<BlockTemplate>(r#"{"version":0,"rules":["a"],"vbavailable":{"b":5},"vbrequired":10,"previousblockhash":"0a00000000000000000000000000000000000000000000000000000000000000","transactions":[{"data":"00010203","txid":null,"hash":null,"depends":null,"fee":null,"sigops":null,"weight":null,"required":false}],"coinbaseaux":{"c":"d"},"coinbasevalue":30,"coinbasetxn":{"data":"555555","txid":"2c00000000000000000000000000000000000000000000000000000000000000","hash":"3700000000000000000000000000000000000000000000000000000000000000","depends":[1],"fee":300,"sigops":400,"weight":500,"required":true},"target":"6400000000000000000000000000000000000000000000000000000000000000","mintime":7,"mutable":["afg"],"noncerange":"00000000ffffffff","sigoplimit":45,"sizelimit":449,"weightlimit":523,"curtime":100,"bits":200,"height":300}"#).unwrap(),
			BlockTemplate {
				version: 0,
				rules: Some(vec!["a".to_owned()]),
				vbavailable: Some(vec![("b".to_owned(), 5)].into_iter().collect()),
				vbrequired: Some(10),
				previousblockhash: H256::from(10),
				transactions: vec![BlockTemplateTransaction {
					data: Bytes("00010203".from_hex().unwrap()),
					txid: None,
					hash: None,
					depends: None,
					fee: None,
					sigops: None,
					weight: None,
					required: false,
				}],
				coinbaseaux: Some(vec![("c".to_owned(), "d".to_owned())].into_iter().collect()),
				coinbasevalue: Some(30),
				coinbasetxn: Some(BlockTemplateTransaction {
					data: Bytes("555555".from_hex().unwrap()),
					txid: Some(H256::from(44)),
					hash: Some(H256::from(55)),
					depends: Some(vec![1]),
					fee: Some(300),
					sigops: Some(400),
					weight: Some(500),
					required: true,
				}),
				target: H256::from(100),
				mintime: Some(7),
				mutable: Some(vec!["afg".to_owned()]),
				noncerange: Some("00000000ffffffff".to_owned()),
				sigoplimit: Some(45),
				sizelimit: Some(449),
				weightlimit: Some(523),
				curtime: 100,
				bits: 200,
				height: 300,
			});
	}
}
