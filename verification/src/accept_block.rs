use network::{ConsensusParams, ConsensusFork};
use crypto::dhash256;
use db::{TransactionOutputProvider, BlockHeaderProvider};
use script;
use ser::Stream;
use sigops::{transaction_sigops, transaction_sigops_cost}	;
use work::block_reward_satoshi;
use duplex_store::DuplexTransactionOutputProvider;
use deployments::BlockDeployments;
use canon::CanonBlock;
use error::{Error, TransactionError};
use timestamp::median_timestamp;

/// Flexible verification of ordered block
pub struct BlockAcceptor<'a> {
	pub finality: BlockFinality<'a>,
	pub serialized_size: BlockSerializedSize<'a>,
	pub sigops: BlockSigops<'a>,
	pub coinbase_claim: BlockCoinbaseClaim<'a>,
	pub coinbase_script: BlockCoinbaseScript<'a>,
	pub witness: BlockWitness<'a>,
}

impl<'a> BlockAcceptor<'a> {
	pub fn new(
		store: &'a TransactionOutputProvider,
		consensus: &'a ConsensusParams,
		block: CanonBlock<'a>,
		height: u32,
		deployments: &'a BlockDeployments<'a>,
		headers: &'a BlockHeaderProvider,
	) -> Self {
		BlockAcceptor {
			finality: BlockFinality::new(block, height, deployments, headers),
			serialized_size: BlockSerializedSize::new(block, consensus, deployments, height),
			coinbase_script: BlockCoinbaseScript::new(block, consensus, height),
			coinbase_claim: BlockCoinbaseClaim::new(block, store, height),
			sigops: BlockSigops::new(block, store, consensus, height),
			witness: BlockWitness::new(block, deployments),
		}
	}

	pub fn check(&self) -> Result<(), Error> {
		self.finality.check()?;
		self.sigops.check()?;
		self.serialized_size.check()?;
		self.coinbase_claim.check()?;
		self.coinbase_script.check()?;
		self.witness.check()?;
		Ok(())
	}
}

pub struct BlockFinality<'a> {
	block: CanonBlock<'a>,
	height: u32,
	csv_active: bool,
	headers: &'a BlockHeaderProvider,
}

impl<'a> BlockFinality<'a> {
	fn new(block: CanonBlock<'a>, height: u32, deployments: &'a BlockDeployments<'a>, headers: &'a BlockHeaderProvider) -> Self {
		let csv_active = deployments.csv();

		BlockFinality {
			block: block,
			height: height,
			csv_active: csv_active,
			headers: headers,
		}
	}

	fn check(&self) -> Result<(), Error> {
		let time_cutoff = if self.csv_active {
			median_timestamp(&self.block.header.raw, self.headers)
		} else {
			self.block.header.raw.time
		};

		if self.block.transactions.iter().all(|tx| tx.raw.is_final_in_block(self.height, time_cutoff)) {
			Ok(())
		} else {
			Err(Error::NonFinalBlock)
		}
	}
}

pub struct BlockSerializedSize<'a> {
	block: CanonBlock<'a>,
	consensus: &'a ConsensusParams,
	height: u32,
	segwit_active: bool,
}

impl<'a> BlockSerializedSize<'a> {
	fn new(block: CanonBlock<'a>, consensus: &'a ConsensusParams, deployments: &'a BlockDeployments<'a>, height: u32) -> Self {
		let segwit_active = deployments.segwit();

		BlockSerializedSize {
			block: block,
			consensus: consensus,
			height: height,
			segwit_active: segwit_active,
		}
	}

	fn check(&self) -> Result<(), Error> {
		let size = self.block.size();

		// block size (without witness) is valid for all forks:
		// before SegWit: it is main check for size
		// after SegWit: without witness data, block size should be <= 1_000_000
		// after BitcoinCash fork: block size is increased to 8_000_000
		// after SegWit2x fork: without witness data, block size should be <= 2_000_000
		if size < self.consensus.fork.min_block_size(self.height) ||
			size > self.consensus.fork.max_block_size(self.height) {
			return Err(Error::Size(size));
		}

		// there's no need to define weight for pre-SegWit blocks
		if self.segwit_active {
			let size_with_witness = self.block.size_with_witness();
			let weight = size * (ConsensusFork::witness_scale_factor() - 1) + size_with_witness;
			if weight > self.consensus.fork.max_block_weight(self.height) {
				return Err(Error::Weight);
			}
		}
		Ok(())
	}
}

pub struct BlockSigops<'a> {
	block: CanonBlock<'a>,
	store: &'a TransactionOutputProvider,
	consensus: &'a ConsensusParams,
	height: u32,
	bip16_active: bool,
}

impl<'a> BlockSigops<'a> {
	fn new(block: CanonBlock<'a>, store: &'a TransactionOutputProvider, consensus: &'a ConsensusParams, height: u32) -> Self {
		let bip16_active = block.header.raw.time >= consensus.bip16_time;

		BlockSigops {
			block: block,
			store: store,
			consensus: consensus,
			height: height,
			bip16_active: bip16_active,
		}
	}

	fn check(&self) -> Result<(), Error> {
		let store = DuplexTransactionOutputProvider::new(self.store, &*self.block);
		let (sigops, sigops_cost) = self.block.transactions.iter()
			.map(|tx| {
				let tx_sigops = transaction_sigops(&tx.raw, &store, self.bip16_active);
				let tx_sigops_cost = transaction_sigops_cost(&tx.raw, &store, tx_sigops);
				(tx_sigops, tx_sigops_cost)
			})
			.fold((0, 0), |acc, (tx_sigops, tx_sigops_cost)| (acc.0 + tx_sigops, acc.1 + tx_sigops_cost));

		// sigops check is valid for all forks:
		// before SegWit: 20_000
		// after SegWit: cost of sigops is sigops * 4 and max cost is 80_000 => max sigops is still 20_000
		// after BitcoinCash fork: 20_000 sigops for each full/partial 1_000_000 bytes of block
		// after SegWit2x fork: cost of sigops is sigops * 4 and max cost is 160_000 => max sigops is 40_000
		let size = self.block.size();
		if sigops > self.consensus.fork.max_block_sigops(self.height, size) {
			return Err(Error::MaximumSigops);
		}

		// sigops check is valid for all forks:
		// before SegWit: no witnesses => cost is sigops * 4 and max cost is 80_000
		// after SegWit: it is main check for sigops
		// after BitcoinCash fork: no witnesses => cost is sigops * 4 and max cost depends on block size
		// after SegWit2x: it is basic check for sigops, limits are increased
		if sigops_cost > self.consensus.fork.max_block_sigops_cost(self.height, size) {
			Err(Error::MaximumSigopsCost)
		} else {
			Ok(())
		}
	}
}

pub struct BlockCoinbaseClaim<'a> {
	block: CanonBlock<'a>,
	store: &'a TransactionOutputProvider,
	height: u32,
}

impl<'a> BlockCoinbaseClaim<'a> {
	fn new(block: CanonBlock<'a>, store: &'a TransactionOutputProvider, height: u32) -> Self {
		BlockCoinbaseClaim {
			block: block,
			store: store,
			height: height,
		}
	}

	fn check(&self) -> Result<(), Error> {
		let store = DuplexTransactionOutputProvider::new(self.store, &*self.block);

		let mut fees: u64 = 0;

		for (tx_idx, tx) in self.block.transactions.iter().enumerate().skip(1) {
			// (1) Total sum of all referenced outputs
			let mut incoming: u64 = 0;
			for input in tx.raw.inputs.iter() {
				let (sum, overflow) = incoming.overflowing_add(
					store.transaction_output(&input.previous_output, tx_idx).map(|o| o.value).unwrap_or(0));
				if overflow {
					return Err(Error::ReferencedInputsSumOverflow);
				}
				incoming = sum;
			}

			// (2) Total sum of all outputs
			let spends = tx.raw.total_spends();

			// Difference between (1) and (2)
			let (difference, overflow) = incoming.overflowing_sub(spends);
			if overflow {
				return Err(Error::Transaction(tx_idx, TransactionError::Overspend))
			}

			// Adding to total fees (with possible overflow)
			let (sum, overflow) = fees.overflowing_add(difference);
			if overflow {
				return Err(Error::TransactionFeesOverflow)
			}

			fees = sum;
		}

		let claim = self.block.transactions[0].raw.total_spends();

		let (reward, overflow) = fees.overflowing_add(block_reward_satoshi(self.height));
		if overflow {
			return Err(Error::TransactionFeeAndRewardOverflow);
		}

		if claim > reward {
			Err(Error::CoinbaseOverspend { expected_max: reward, actual: claim })
		} else {
			Ok(())
		}
	}
}

pub struct BlockCoinbaseScript<'a> {
	block: CanonBlock<'a>,
	bip34_active: bool,
	height: u32,
}

impl<'a> BlockCoinbaseScript<'a> {
	fn new(block: CanonBlock<'a>, consensus_params: &ConsensusParams, height: u32) -> Self {
		BlockCoinbaseScript {
			block: block,
			bip34_active: height >= consensus_params.bip34_height,
			height: height,
		}
	}

	fn check(&self) -> Result<(), Error> {
		if !self.bip34_active {
			return Ok(())
		}

		let prefix = script::Builder::default()
			.push_num(self.height.into())
			.into_script();

		let matches = self.block.transactions.first()
			.and_then(|tx| tx.raw.inputs.first())
			.map(|input| input.script_sig.starts_with(&prefix))
			.unwrap_or(false);

		if matches {
			Ok(())
		} else {
			Err(Error::CoinbaseScript)
		}
	}
}

pub struct BlockWitness<'a> {
	block: CanonBlock<'a>,
	segwit_active: bool,
}

impl<'a> BlockWitness<'a> {
	fn new(block: CanonBlock<'a>, deployments: &'a BlockDeployments<'a>) -> Self {
		let segwit_active = deployments.segwit();

		BlockWitness {
			block: block,
			segwit_active: segwit_active,
		}
	}

	fn check(&self) -> Result<(), Error> {
		if !self.segwit_active {
			return Ok(());
		}

		// check witness from coinbase transaction
		let mut has_witness = false;
		if let Some(coinbase) = self.block.transactions.first() {
			let commitment = coinbase.raw.outputs.iter().rev()
				.find(|output| script::is_witness_commitment_script(&output.script_pubkey));
			if let Some(commitment) = commitment {
				let witness_merkle_root = self.block.raw().witness_merkle_root();
				if coinbase.raw.inputs.get(0).map(|i| i.script_witness.len()).unwrap_or_default() != 1 ||
					coinbase.raw.inputs[0].script_witness[0].len() != 32 {
					return Err(Error::WitnessInvalidNonceSize);
				}

				let mut stream = Stream::new();
				stream.append(&witness_merkle_root);
				stream.append_slice(&coinbase.raw.inputs[0].script_witness[0]);
				let hash_witness = dhash256(&stream.out());

				if hash_witness != commitment.script_pubkey[6..].into() {
					return Err(Error::WitnessMerkleCommitmentMismatch);
				}

				has_witness = true;
			}
		}

		// witness commitment is required when block contains transactions with witness
		if !has_witness && self.block.transactions.iter().any(|tx| tx.raw.has_witness()) {
			return Err(Error::UnexpectedWitness);
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	extern crate test_data;

	use {Error, CanonBlock};
	use super::BlockCoinbaseScript;

	#[test]
	fn test_block_coinbase_script() {
		// transaction from block 461373
		// https://blockchain.info/rawtx/7cf05175ce9c8dbfff9aafa8263edc613fc08f876e476553009afcf7e3868a0c?format=hex
		let tx = "01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff3f033d0a070004b663ec58049cba630608733867a0787a02000a425720537570706f727420384d200a666973686572206a696e78696e092f425720506f6f6c2fffffffff01903d9d4e000000001976a914721afdf638d570285d02d3076d8be6a03ee0794d88ac00000000".into();
		let block_number = 461373;
		let block = test_data::block_builder()
			.with_transaction(tx)
			.header().build()
			.build()
			.into();

		let coinbase_script_validator = BlockCoinbaseScript {
			block: CanonBlock::new(&block),
			bip34_active: true,
			height: block_number,
		};

		assert_eq!(coinbase_script_validator.check(), Ok(()));

		let coinbase_script_validator2 = BlockCoinbaseScript {
			block: CanonBlock::new(&block),
			bip34_active: true,
			height: block_number - 1,
		};

		assert_eq!(coinbase_script_validator2.check(), Err(Error::CoinbaseScript));
	}
}
