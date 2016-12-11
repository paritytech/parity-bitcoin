use primitives::hash::H256;
use db::{TransactionMetaProvider, PreviousTransactionOutputProvider};
use network::{Magic, ConsensusParams};
use duplex_store::{DuplexTransactionOutputProvider};
use canon::CanonTransaction;
use constants::COINBASE_MATURITY;
use error::TransactionError;

pub struct TransactionAcceptor<'a> {
	pub bip30: TransactionBip30<'a>,
	pub missing_inputs: TransactionMissingInputs<'a>,
	pub maturity: TransactionMaturity<'a>,
	pub overspent: TransactionOverspent<'a>,
}

impl<'a> TransactionAcceptor<'a> {
	pub fn new(
		// transactions meta
		// in case of block validation, it's only current block,
		// TODO: in case of memory pool it should be db and memory pool
		meta_store: &'a TransactionMetaProvider,
		// previous transaction outputs
		// in case of block validation, that's database and currently processed block
		// in case of memory pool it should be db and memory pool
		prevout_store: DuplexTransactionOutputProvider<'a>,
		network: Magic,
		transaction: CanonTransaction<'a>,
		block_hash: &'a H256,
		height: u32
	) -> Self {
		TransactionAcceptor {
			bip30: TransactionBip30::new(transaction, meta_store, network.consensus_params(), block_hash, height),
			missing_inputs: TransactionMissingInputs::new(transaction, prevout_store),
			maturity: TransactionMaturity::new(transaction, meta_store, height),
			overspent: TransactionOverspent::new(transaction, prevout_store),
		}
	}

	pub fn check(&self) -> Result<(), TransactionError> {
		try!(self.bip30.check());
		try!(self.missing_inputs.check());
		// TODO: double spends
		try!(self.maturity.check());
		try!(self.overspent.check());
		Ok(())
	}
}

pub trait TransactionRule {
	fn check(&self) -> Result<(), TransactionError>;
}

pub struct TransactionBip30<'a> {
	transaction: CanonTransaction<'a>,
	store: &'a TransactionMetaProvider,
	consensus_params: ConsensusParams,
	block_hash: &'a H256,
	height: u32,
}

impl<'a> TransactionBip30<'a> {
	fn new(transaction: CanonTransaction<'a>, store: &'a TransactionMetaProvider, consensus_params: ConsensusParams, block_hash: &'a H256, height: u32) -> Self {
		TransactionBip30 {
			transaction: transaction,
			store: store,
			consensus_params: consensus_params,
			block_hash: block_hash,
			height: height,
		}
	}
}

impl<'a> TransactionRule for TransactionBip30<'a> {
	fn check(&self) -> Result<(), TransactionError> {
		// we allow optionals here, cause previous output may be a part of current block
		// yet, we do not need to check current block, cause duplicated transactions
		// in the same block are also forbidden
		//
		// update*
		// TODO:
		// There is a potential consensus failure here, cause transaction before this one
		// may have fully spent the output, and we, by checking only storage, have no knowladge
		// of it
		match self.store.transaction_meta(&self.transaction.hash) {
			Some(ref meta) if !meta.is_fully_spent() && !self.consensus_params.is_bip30_exception(self.block_hash, self.height) => {
				Err(TransactionError::UnspentTransactionWithTheSameHash)
			},
			_ => Ok(())
		}
	}
}

pub struct TransactionMissingInputs<'a> {
	transaction: CanonTransaction<'a>,
	store: DuplexTransactionOutputProvider<'a>,
}

impl<'a> TransactionMissingInputs<'a> {
	fn new(transaction: CanonTransaction<'a>, store: DuplexTransactionOutputProvider<'a>) -> Self {
		TransactionMissingInputs {
			transaction: transaction,
			store: store,
		}
	}
}

impl<'a> TransactionRule for TransactionMissingInputs<'a> {
	fn check(&self) -> Result<(), TransactionError> {
		let missing_index = self.transaction.raw.inputs.iter()
			.position(|input| {
				let is_not_null = !input.previous_output.is_null();
				let is_missing = self.store.previous_transaction_output(&input.previous_output).is_none();
				is_not_null && is_missing
			});

		match missing_index {
			Some(index) => Err(TransactionError::Input(index)),
			None => Ok(())
		}
	}
}

pub struct TransactionMaturity<'a> {
	transaction: CanonTransaction<'a>,
	store: &'a TransactionMetaProvider,
	height: u32,
}

impl<'a> TransactionMaturity<'a> {
	fn new(transaction: CanonTransaction<'a>, store: &'a TransactionMetaProvider, height: u32) -> Self {
		TransactionMaturity {
			transaction: transaction,
			store: store,
			height: height,
		}
	}
}

impl<'a> TransactionRule for TransactionMaturity<'a> {
	fn check(&self) -> Result<(), TransactionError> {
		// TODO: this is should also fail when we are trying to spend current block coinbase
		let immature_spend = self.transaction.raw.inputs.iter()
			.any(|input| match self.store.transaction_meta(&input.previous_output.hash) {
				Some(ref meta) if meta.is_coinbase() && self.height < meta.height() + COINBASE_MATURITY => true,
				_ => false,
			});

		if immature_spend {
			Err(TransactionError::Maturity)
		} else {
			Ok(())
		}
	}
}

pub struct TransactionOverspent<'a> {
	transaction: CanonTransaction<'a>,
	store: DuplexTransactionOutputProvider<'a>,
}

impl<'a> TransactionOverspent<'a> {
	fn new(transaction: CanonTransaction<'a>, store: DuplexTransactionOutputProvider<'a>) -> Self {
		TransactionOverspent {
			transaction: transaction,
			store: store,
		}
	}
}

impl<'a> TransactionRule for TransactionOverspent<'a> {
	fn check(&self) -> Result<(), TransactionError> {
		if self.transaction.raw.is_coinbase() {
			return Ok(());
		}

		let available = self.transaction.raw.inputs.iter()
			.map(|input| self.store.previous_transaction_output(&input.previous_output).map(|o| o.value).unwrap_or(0))
			.sum::<u64>();

		let spends = self.transaction.raw.total_spends();

		if spends > available {
			Err(TransactionError::Overspend)
		} else {
			Ok(())
		}
	}
}
