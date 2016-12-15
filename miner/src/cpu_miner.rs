use byteorder::{WriteBytesExt, LittleEndian};
use primitives::bytes::Bytes;
use primitives::hash::H256;
use primitives::uint::U256;
use chain::{merkle_root, Transaction};
use crypto::dhash256;
use ser::Stream;
use block_assembler::BlockTemplate;
use verification::is_valid_proof_of_work_hash;

/// Instead of serializing `BlockHeader` from scratch over and over again,
/// let's keep it serialized in memory and replace needed bytes
struct BlockHeaderBytes {
	data: Bytes,
}

impl BlockHeaderBytes {
	/// Creates new instance of block header bytes.
	fn new(version: u32, previous_header_hash: H256, nbits: u32) -> Self {
		let merkle_root_hash = H256::default();
		let time = 0u32;
		let nonce = 0u32;

		let mut stream = Stream::default();
		stream
			.append(&version)
			.append(&previous_header_hash)
			.append(&merkle_root_hash)
			.append(&time)
			.append(&nbits)
			.append(&nonce);

		BlockHeaderBytes {
			data: stream.out(),
		}
	}

	/// Set merkle root hash
	fn set_merkle_root_hash(&mut self, hash: &H256) {
		let mut merkle_bytes: &mut [u8] = &mut self.data[4 + 32..4 + 32 + 32];
		merkle_bytes.copy_from_slice(&**hash);
	}

	/// Set block header time
	fn set_time(&mut self, time: u32) {
		let mut time_bytes: &mut [u8] = &mut self.data[4 + 32 + 32..];
		time_bytes.write_u32::<LittleEndian>(time).unwrap();
	}

	/// Set block header nonce
	fn set_nonce(&mut self, nonce: u32) {
		let mut nonce_bytes: &mut [u8] = &mut self.data[4 + 32 + 32 + 4 + 4..];
		nonce_bytes.write_u32::<LittleEndian>(nonce).unwrap();
	}

	/// Returns block header hash
	fn hash(&self) -> H256 {
		dhash256(&self.data)
	}
}

/// This trait should be implemented by coinbase transaction.
pub trait CoinbaseTransaction {
	/// Protocols like stratum limit number of extranonce bytes.
	/// This function informs miner about maximum size of extra nonce.
	fn max_extranonce(&self) -> U256;
	/// Should be used to increase number of hash possibities for miner
	fn set_extranonce(&mut self, extranocne: &U256);
	/// Returns transaction hash
	fn hash(&self) -> H256;
	/// Coverts transaction into raw bytes
	fn drain(self) -> Transaction;
}

/// Cpu miner solution.
pub struct Solution {
	/// Block header nonce.
	pub nonce: u32,
	/// Coinbase transaction extra nonce (modyfiable by miner).
	pub extranonce: U256,
	/// Block header time.
	pub time: u32,
	/// Coinbase transaction (extranonce is already set).
	pub coinbase_transaction: Transaction,
}

/// Simple bitcoin cpu miner.
///
/// First it tries to find solution by changing block header nonce.
/// Once all nonce values have been tried, it increases extranonce.
/// Once all of them have been tried (quite unlikely on cpu ;),
/// and solution still hasn't been found it returns None.
/// It's possible to also experiment with time, but I find it pointless
/// to implement on CPU.
pub fn find_solution<T>(block: BlockTemplate, mut coinbase_transaction: T) -> Option<Solution> where T: CoinbaseTransaction {
	let max_extranonce = coinbase_transaction.max_extranonce();
	let mut extranonce = U256::default();

	let mut header_bytes = BlockHeaderBytes::new(block.version, block.previous_header_hash, block.nbits);
	// update header with time
	header_bytes.set_time(block.time);

	while extranonce < max_extranonce {
		// update coinbase transaction with new extranonce
		coinbase_transaction.set_extranonce(&extranonce);

		// recalculate merkle root hash
		let coinbase_hash = coinbase_transaction.hash();
		let mut merkle_tree = vec![&coinbase_hash];
		merkle_tree.extend(block.transactions.iter().map(|tx| &tx.hash));
		let merkle_root_hash = merkle_root(&merkle_tree);

		// update header with new merkle root hash
		header_bytes.set_merkle_root_hash(&merkle_root_hash);

		for nonce in 0..(u32::max_value() as u64 + 1) {
			// update §
			header_bytes.set_nonce(nonce as u32);
			let hash = header_bytes.hash();
			if is_valid_proof_of_work_hash(block.nbits.into(), &hash) {
				let solution = Solution {
					nonce: nonce as u32,
					extranonce: extranonce,
					time: block.time,
					coinbase_transaction: coinbase_transaction.drain(),
				};

				return Some(solution);
			}
		}

		extranonce = extranonce + 1.into();
	}

	None
}
