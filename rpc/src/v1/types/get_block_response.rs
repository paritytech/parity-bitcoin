use super::bytes::Bytes;
use super::hash::H256;
use super::raw_block::RawBlock;

/// Response to getblock RPC request
#[derive(Debug, Serialize, Deserialize)]
pub enum GetBlockResponse {
	/// When asking for short response
	Short(RawBlock),
	/// When asking for verbose response
	Verbose(Block),
}

/// Verbose block information
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Block {
	/// Block hash
	pub hash: H256,
	/// Number of confirmations. -1 if block is on the side chain
	pub confirmations: i64,
	/// Block size
	pub size: u32,
	/// Block size, excluding witness data
	pub strippedsize: u32,
	/// Block weight
	pub weight: u32,
	/// Block height
	pub height: u32,
	/// Block version
	pub version: u32,
	/// Block version as hex
	#[serde(rename = "versionHex")]
	pub version_hex: Bytes,
	/// Merkle root of this block
	pub merkleroot: H256,
	/// Transactions ids
	pub tx: Vec<H256>,
	/// Block time in seconds since epoch (Jan 1 1970 GMT)
	pub time: u32,
	/// Median block time in seconds since epoch (Jan 1 1970 GMT)
	pub mediantime: u32,
	/// Block nonce
	pub nonce: u32,
	/// Block nbits
	pub bits: u32,
	/// Block difficulty
	pub difficulty: f64,
	/// Expected number of hashes required to produce the chain up to this block (in hex)
	pub chainwork: H256,
	/// Hash of previous block
	pub previousblockhash: Option<H256>,
	/// Hash of next block
	pub nextblockhash: Option<H256>,
}

#[cfg(test)]
mod tests {
	use super::super::bytes::Bytes;
	use super::super::hash::H256;
	use serde_json;
	use super::*;

	#[test]
	fn block_serialize() {
		let block = Block {
			version_hex: Bytes::new(vec![0]),
			..Default::default()
		};
		assert_eq!(serde_json::to_string(&block).unwrap(), r#"{"hash":"0000000000000000000000000000000000000000000000000000000000000000","confirmations":0,"size":0,"strippedsize":0,"weight":0,"height":0,"version":0,"versionHex":"00","merkleroot":"0000000000000000000000000000000000000000000000000000000000000000","tx":[],"time":0,"mediantime":0,"nonce":0,"bits":0,"difficulty":0.0,"chainwork":"0000000000000000000000000000000000000000000000000000000000000000","previousblockhash":null,"nextblockhash":null}"#);

		let block = Block {
			hash: H256::from(1),
			confirmations: -1,
			size: 500000,
			strippedsize: 444444,
			weight: 5236235,
			height: 3513513,
			version: 1,
			version_hex: Bytes::new(vec![1]),
			merkleroot: H256::from(2),
			tx: vec![H256::from(3), H256::from(4)],
			time: 111,
			mediantime: 100,
			nonce: 124,
			bits: 13513,
			difficulty: 555.555,
			chainwork: H256::from(3),
			previousblockhash: Some(H256::from(4)),
			nextblockhash: Some(H256::from(5)),
		};
		assert_eq!(serde_json::to_string(&block).unwrap(), r#"{"hash":"0100000000000000000000000000000000000000000000000000000000000000","confirmations":-1,"size":500000,"strippedsize":444444,"weight":5236235,"height":3513513,"version":1,"versionHex":"01","merkleroot":"0200000000000000000000000000000000000000000000000000000000000000","tx":["0300000000000000000000000000000000000000000000000000000000000000","0400000000000000000000000000000000000000000000000000000000000000"],"time":111,"mediantime":100,"nonce":124,"bits":13513,"difficulty":555.555,"chainwork":"0300000000000000000000000000000000000000000000000000000000000000","previousblockhash":"0400000000000000000000000000000000000000000000000000000000000000","nextblockhash":"0500000000000000000000000000000000000000000000000000000000000000"}"#);
	}

	#[test]
	fn block_deserialize() {
		let block = Block {
			version_hex: Bytes::new(vec![0]),
			..Default::default()
		};
		assert_eq!(
			serde_json::from_str::<Block>(r#"{"hash":"0000000000000000000000000000000000000000000000000000000000000000","confirmations":0,"size":0,"strippedsize":0,"weight":0,"height":0,"version":0,"versionHex":"00","merkleroot":"0000000000000000000000000000000000000000000000000000000000000000","tx":[],"time":0,"mediantime":0,"nonce":0,"bits":0,"difficulty":0.0,"chainwork":"0000000000000000000000000000000000000000000000000000000000000000","previousblockhash":null,"nextblockhash":null}"#).unwrap(),
			block);

		let block = Block {
			hash: H256::from(1),
			confirmations: -1,
			size: 500000,
			strippedsize: 444444,
			weight: 5236235,
			height: 3513513,
			version: 1,
			version_hex: Bytes::new(vec![1]),
			merkleroot: H256::from(2),
			tx: vec![H256::from(3), H256::from(4)],
			time: 111,
			mediantime: 100,
			nonce: 124,
			bits: 13513,
			difficulty: 555.555,
			chainwork: H256::from(3),
			previousblockhash: Some(H256::from(4)),
			nextblockhash: Some(H256::from(5)),
		};
		assert_eq!(
			serde_json::from_str::<Block>(r#"{"hash":"0100000000000000000000000000000000000000000000000000000000000000","confirmations":-1,"size":500000,"strippedsize":444444,"weight":5236235,"height":3513513,"version":1,"versionHex":"01","merkleroot":"0200000000000000000000000000000000000000000000000000000000000000","tx":["0300000000000000000000000000000000000000000000000000000000000000","0400000000000000000000000000000000000000000000000000000000000000"],"time":111,"mediantime":100,"nonce":124,"bits":13513,"difficulty":555.555,"chainwork":"0300000000000000000000000000000000000000000000000000000000000000","previousblockhash":"0400000000000000000000000000000000000000000000000000000000000000","nextblockhash":"0500000000000000000000000000000000000000000000000000000000000000"}"#).unwrap(),
			block);
	}
}
