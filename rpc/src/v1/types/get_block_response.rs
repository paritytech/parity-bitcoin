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
#[derive(Debug, Serialize, Deserialize)]
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
	#[test]
	fn block_serialize() {

	}
}