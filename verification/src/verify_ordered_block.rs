use std::ops;
use network::{Magic, ConsensusParams};
use db::{SharedStore, IndexedBlock, PreviousTransactionOutputProvider, BlockHeaderProvider};
use sigops::{StoreWithUnretainedOutputs, transaction_sigops};
use utils::{work_required, block_reward_satoshi};
use error::Error;

// imports to rethink
use chain_verifier::MAX_BLOCK_SIGOPS;

const EXPECT_ORDERED: &'static str = "Block ancestors expected to be found in database";

/// Flexible verification of ordered block
pub struct OrderedBlockVerifier<'a> {
	pub finality: BlockFinality<'a>,
	pub sigops: BlockSigops<'a>,
	pub work: BlockWork<'a>,
	pub coinbase_claim: BlockCoinbaseClaim<'a>,
}

impl<'a> OrderedBlockVerifier<'a> {
	pub fn new(store: &'a SharedStore, network: Magic, block: OrderedBlock<'a>, height: u32) -> Self {
		let params = network.consensus_params();
		OrderedBlockVerifier {
			finality: BlockFinality::new(block, height),
			sigops: BlockSigops::new(block, store.as_previous_transaction_output_provider(), params),
			work: BlockWork::new(block, store.as_block_header_provider(), height, network),
			coinbase_claim: BlockCoinbaseClaim::new(block, store.as_previous_transaction_output_provider(), height),
		}
	}

	pub fn check(&self) -> Result<(), Error> {
		try!(self.finality.check());
		try!(self.sigops.check());
		try!(self.work.check());
		try!(self.coinbase_claim.check());
		Ok(())
	}
}

/// Blocks whose parents are known to be in the chain
#[derive(Clone, Copy)]
pub struct OrderedBlock<'a> {
	block: &'a IndexedBlock,
}

impl<'a> OrderedBlock<'a> {
	pub fn new(block: &'a IndexedBlock) -> Self {
		OrderedBlock {
			block: block,
		}
	}
}

impl<'a> ops::Deref for OrderedBlock<'a> {
	type Target = IndexedBlock;

	fn deref(&self) -> &Self::Target {
		self.block
	}
}

trait OrderedBlockRule {
	/// If verification fails returns an error
	fn check(&self) -> Result<(), Error>;
}

pub struct BlockFinality<'a> {
	block: OrderedBlock<'a>,
	height: u32,
}

impl<'a> BlockFinality<'a> {
	fn new(block: OrderedBlock<'a>, height: u32) -> Self {
		BlockFinality {
			block: block,
			height: height,
		}
	}
}

impl<'a> OrderedBlockRule for BlockFinality<'a> {
	fn check(&self) -> Result<(), Error> {
		if self.block.is_final(self.height) {
			Ok(())
		} else {
			Err(Error::NonFinalBlock)
		}
	}
}

pub struct BlockSigops<'a> {
	block: OrderedBlock<'a>,
	store: &'a PreviousTransactionOutputProvider,
	consensus_params: ConsensusParams,
}

impl<'a> BlockSigops<'a> {
	fn new(block: OrderedBlock<'a>, store: &'a PreviousTransactionOutputProvider, consensus_params: ConsensusParams) -> Self {
		BlockSigops {
			block: block,
			store: store,
			consensus_params: consensus_params,
		}
	}
}

impl<'a> OrderedBlockRule for BlockSigops<'a> {
	fn check(&self) -> Result<(), Error> {
		let store = StoreWithUnretainedOutputs::new(self.store, &*self.block);
		let bip16_active = self.block.header.raw.time >= self.consensus_params.bip16_time;
		let sigops = self.block.transactions.iter()
			.map(|tx| transaction_sigops(&tx.raw, &store, bip16_active).expect(EXPECT_ORDERED))
			.sum::<usize>();

		if sigops > MAX_BLOCK_SIGOPS {
			Err(Error::MaximumSigops)
		} else {
			Ok(())
		}
	}
}

pub struct BlockWork<'a> {
	block: OrderedBlock<'a>,
	store: &'a BlockHeaderProvider,
	height: u32,
	network: Magic,
}

impl<'a> BlockWork<'a> {
	fn new(block: OrderedBlock<'a>, store: &'a BlockHeaderProvider, height: u32, network: Magic) -> Self {
		BlockWork {
			block: block,
			store: store,
			height: height,
			network: network,
		}
	}
}

impl<'a> OrderedBlockRule for BlockWork<'a> {
	fn check(&self) -> Result<(), Error> {
		let previous_header_hash = self.block.header.raw.previous_header_hash.clone();
		let time = self.block.header.raw.time;
		let work = work_required(previous_header_hash, time, self.height, self.store, self.network);
		if work == self.block.header.raw.bits {
			Ok(())
		} else {
			Err(Error::Difficulty)
		}
	}
}

pub struct BlockCoinbaseClaim<'a> {
	block: OrderedBlock<'a>,
	store: &'a PreviousTransactionOutputProvider,
	height: u32,
}

impl<'a> BlockCoinbaseClaim<'a> {
	fn new(block: OrderedBlock<'a>, store: &'a PreviousTransactionOutputProvider, height: u32) -> Self {
		BlockCoinbaseClaim {
			block: block,
			store: store,
			height: height,
		}
	}
}

impl<'a> OrderedBlockRule for BlockCoinbaseClaim<'a> {
	fn check(&self) -> Result<(), Error> {
		let store = StoreWithUnretainedOutputs::new(self.store, &*self.block);
		let total_outputs = self.block.transactions.iter()
			.skip(1)
			.flat_map(|tx| tx.raw.inputs.iter())
			.map(|input| store.previous_transaction_output(&input.previous_output).expect(EXPECT_ORDERED))
			.map(|output| output.value)
			.sum::<u64>();

		let total_inputs = self.block.transactions.iter()
			.skip(1)
			.map(|tx| tx.raw.total_spends())
			.sum::<u64>();

		let claim = self.block.transactions[0].raw.total_spends();
		let (fees, overflow) = total_outputs.overflowing_sub(total_inputs);
		let reward = fees + block_reward_satoshi(self.height);
		if overflow || claim > reward {
			Err(Error::CoinbaseOverspend { expected_max: reward, actual: claim })
		} else {
			Ok(())
		}
	}
}
