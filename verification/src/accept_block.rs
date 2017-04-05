use network::{Magic, ConsensusParams};
use db::PreviousTransactionOutputProvider;
use sigops::transaction_sigops;
use work::block_reward_satoshi;
use duplex_store::DuplexTransactionOutputProvider;
use canon::CanonBlock;
use constants::MAX_BLOCK_SIGOPS;
use error::{Error, TransactionError};

/// Flexible verification of ordered block
pub struct BlockAcceptor<'a> {
	pub finality: BlockFinality<'a>,
	pub sigops: BlockSigops<'a>,
	pub coinbase_claim: BlockCoinbaseClaim<'a>,
}

impl<'a> BlockAcceptor<'a> {
	pub fn new(store: &'a PreviousTransactionOutputProvider, network: Magic, block: CanonBlock<'a>, height: u32) -> Self {
		let params = network.consensus_params();
		BlockAcceptor {
			finality: BlockFinality::new(block, height),
			sigops: BlockSigops::new(block, store, params, MAX_BLOCK_SIGOPS),
			coinbase_claim: BlockCoinbaseClaim::new(block, store, height),
		}
	}

	pub fn check(&self) -> Result<(), Error> {
		try!(self.finality.check());
		try!(self.sigops.check());
		try!(self.coinbase_claim.check());
		Ok(())
	}
}

trait BlockRule {
	/// If verification fails returns an error
	fn check(&self) -> Result<(), Error>;
}

pub struct BlockFinality<'a> {
	block: CanonBlock<'a>,
	height: u32,
}

impl<'a> BlockFinality<'a> {
	fn new(block: CanonBlock<'a>, height: u32) -> Self {
		BlockFinality {
			block: block,
			height: height,
		}
	}
}

impl<'a> BlockRule for BlockFinality<'a> {
	fn check(&self) -> Result<(), Error> {
		if self.block.is_final(self.height) {
			Ok(())
		} else {
			Err(Error::NonFinalBlock)
		}
	}
}

pub struct BlockSigops<'a> {
	block: CanonBlock<'a>,
	store: &'a PreviousTransactionOutputProvider,
	consensus_params: ConsensusParams,
	max_sigops: usize,
}

impl<'a> BlockSigops<'a> {
	fn new(block: CanonBlock<'a>, store: &'a PreviousTransactionOutputProvider, consensus_params: ConsensusParams, max_sigops: usize) -> Self {
		BlockSigops {
			block: block,
			store: store,
			consensus_params: consensus_params,
			max_sigops: max_sigops,
		}
	}
}

impl<'a> BlockRule for BlockSigops<'a> {
	fn check(&self) -> Result<(), Error> {
		let store = DuplexTransactionOutputProvider::new(self.store, &*self.block);
		let bip16_active = self.block.header.raw.time >= self.consensus_params.bip16_time;
		let sigops = self.block.transactions.iter()
			.map(|tx| transaction_sigops(&tx.raw, &store, bip16_active))
			.sum::<usize>();

		if sigops > self.max_sigops {
			Err(Error::MaximumSigops)
		} else {
			Ok(())
		}
	}
}

pub struct BlockCoinbaseClaim<'a> {
	block: CanonBlock<'a>,
	store: &'a PreviousTransactionOutputProvider,
	height: u32,
}

impl<'a> BlockCoinbaseClaim<'a> {
	fn new(block: CanonBlock<'a>, store: &'a PreviousTransactionOutputProvider, height: u32) -> Self {
		BlockCoinbaseClaim {
			block: block,
			store: store,
			height: height,
		}
	}
}

impl<'a> BlockRule for BlockCoinbaseClaim<'a> {
	fn check(&self) -> Result<(), Error> {
		let store = DuplexTransactionOutputProvider::new(self.store, &*self.block);

		let mut fees: u64 = 0;

		for (tx_idx, tx) in self.block.transactions.iter().enumerate().skip(1) {
			// (1) Total sum of all referenced outputs
			let mut incoming: u64 = 0;
			for input in tx.raw.inputs.iter() {
				let (sum, overflow) = incoming.overflowing_add(
					store.previous_transaction_output(&input.previous_output, tx_idx).map(|o| o.value).unwrap_or(0));
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
