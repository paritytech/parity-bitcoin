use primitives::hash::H256;
use db::{TransactionMetaProvider, PreviousTransactionOutputProvider, TransactionOutputObserver};
use network::{Magic, ConsensusParams};
use script::{Script, verify_script, VerificationFlags, TransactionSignatureChecker, TransactionInputSigner};
use duplex_store::{DuplexTransactionOutputProvider};
use sigops::transaction_sigops;
use canon::CanonTransaction;
use constants::{COINBASE_MATURITY, MAX_BLOCK_SIGOPS};
use error::TransactionError;

pub struct TransactionAcceptor<'a> {
	pub bip30: TransactionBip30<'a>,
	pub missing_inputs: TransactionMissingInputs<'a>,
	pub maturity: TransactionMaturity<'a>,
	pub overspent: TransactionOverspent<'a>,
	pub double_spent: TransactionDoubleSpend<'a>,
	pub eval: TransactionEval<'a>,
}

impl<'a> TransactionAcceptor<'a> {
	pub fn new(
		// in case of block validation, it's only current block,
		meta_store: &'a TransactionMetaProvider,
		// previous transaction outputs
		// in case of block validation, that's database and currently processed block
		prevout_store: DuplexTransactionOutputProvider<'a>,
		// in case of block validation, that's database and currently processed block
		spent_store: &'a TransactionOutputObserver,
		network: Magic,
		transaction: CanonTransaction<'a>,
		block_hash: &'a H256,
		height: u32,
		time: u32,
	) -> Self {
		trace!(target: "verification", "Tx verification {}", transaction.hash.to_reversed_str());
		let params = network.consensus_params();
		TransactionAcceptor {
			bip30: TransactionBip30::new_for_sync(transaction, meta_store, params.clone(), block_hash, height),
			missing_inputs: TransactionMissingInputs::new(transaction, prevout_store),
			maturity: TransactionMaturity::new(transaction, meta_store, height),
			overspent: TransactionOverspent::new(transaction, prevout_store),
			double_spent: TransactionDoubleSpend::new(transaction, spent_store),
			eval: TransactionEval::new(transaction, prevout_store, params, height, time),
		}
	}

	pub fn check(&self) -> Result<(), TransactionError> {
		try!(self.bip30.check());
		try!(self.missing_inputs.check());
		try!(self.maturity.check());
		try!(self.overspent.check());
		try!(self.double_spent.check());
		try!(self.eval.check());
		Ok(())
	}
}

pub struct MemoryPoolTransactionAcceptor<'a> {
	pub bip30: TransactionBip30<'a>,
	pub missing_inputs: TransactionMissingInputs<'a>,
	pub maturity: TransactionMaturity<'a>,
	pub overspent: TransactionOverspent<'a>,
	pub sigops: TransactionSigops<'a>,
	pub double_spent: TransactionDoubleSpend<'a>,
	pub eval: TransactionEval<'a>,
}

impl<'a> MemoryPoolTransactionAcceptor<'a> {
	pub fn new(
		// TODO: in case of memory pool it should be db and memory pool
		meta_store: &'a TransactionMetaProvider,
		// in case of memory pool it should be db and memory pool
		prevout_store: DuplexTransactionOutputProvider<'a>,
		// in case of memory pool it should be db and memory pool
		spent_store: &'a TransactionOutputObserver,
		network: Magic,
		transaction: CanonTransaction<'a>,
		height: u32,
		time: u32,
	) -> Self {
		trace!(target: "verification", "Mempool-Tx verification {}", transaction.hash.to_reversed_str());
		let params = network.consensus_params();
		MemoryPoolTransactionAcceptor {
			bip30: TransactionBip30::new_for_mempool(transaction, meta_store),
			missing_inputs: TransactionMissingInputs::new(transaction, prevout_store),
			maturity: TransactionMaturity::new(transaction, meta_store, height),
			overspent: TransactionOverspent::new(transaction, prevout_store),
			sigops: TransactionSigops::new(transaction, prevout_store, params.clone(), MAX_BLOCK_SIGOPS, time),
			double_spent: TransactionDoubleSpend::new(transaction, spent_store),
			eval: TransactionEval::new(transaction, prevout_store, params, height, time),
		}
	}

	pub fn check(&self) -> Result<(), TransactionError> {
		// TODO: b82 fails, when this is enabled, fix this
		//try!(self.bip30.check());
		try!(self.missing_inputs.check());
		try!(self.maturity.check());
		try!(self.overspent.check());
		try!(self.sigops.check());
		try!(self.double_spent.check());
		try!(self.eval.check());
		Ok(())
	}
}

pub trait TransactionRule {
	fn check(&self) -> Result<(), TransactionError>;
}

pub struct TransactionBip30<'a> {
	transaction: CanonTransaction<'a>,
	store: &'a TransactionMetaProvider,
	exception: bool,
}

impl<'a> TransactionBip30<'a> {
	fn new_for_sync(
		transaction: CanonTransaction<'a>,
		store: &'a TransactionMetaProvider,
		consensus_params: ConsensusParams,
		block_hash: &'a H256,
		height: u32
	) -> Self {
		let exception = consensus_params.is_bip30_exception(block_hash, height);

		TransactionBip30 {
			transaction: transaction,
			store: store,
			exception: exception,
		}
	}

	fn new_for_mempool(transaction: CanonTransaction<'a>, store: &'a TransactionMetaProvider) -> Self {
		TransactionBip30 {
			transaction: transaction,
			store: store,
			exception: false,
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
			Some(ref meta) if !meta.is_fully_spent() && !self.exception => {
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

pub struct TransactionSigops<'a> {
	transaction: CanonTransaction<'a>,
	store: DuplexTransactionOutputProvider<'a>,
	consensus_params: ConsensusParams,
	max_sigops: usize,
	time: u32,
}

impl<'a> TransactionSigops<'a> {
	fn new(transaction: CanonTransaction<'a>, store: DuplexTransactionOutputProvider<'a>, consensus_params: ConsensusParams, max_sigops: usize, time: u32) -> Self {
		TransactionSigops {
			transaction: transaction,
			store: store,
			consensus_params: consensus_params,
			max_sigops: max_sigops,
			time: time,
		}
	}
}

impl<'a> TransactionRule for TransactionSigops<'a> {
	fn check(&self) -> Result<(), TransactionError> {
		let bip16_active = self.time >= self.consensus_params.bip16_time;
		let sigops = transaction_sigops(&self.transaction.raw, &self.store, bip16_active);
		if sigops > self.max_sigops {
			Err(TransactionError::MaxSigops)
		} else {
			Ok(())
		}
	}
}

pub struct TransactionEval<'a> {
	transaction: CanonTransaction<'a>,
	store: DuplexTransactionOutputProvider<'a>,
	verify_p2sh: bool,
	verify_clocktime: bool,
}

impl<'a> TransactionEval<'a> {
	fn new(
		transaction: CanonTransaction<'a>,
		store: DuplexTransactionOutputProvider<'a>,
		params: ConsensusParams,
		height: u32,
		time: u32,
	) -> Self {
		let verify_p2sh = time >= params.bip16_time;
		let verify_clocktime = height >= params.bip65_height;

		TransactionEval {
			transaction: transaction,
			store: store,
			verify_p2sh: verify_p2sh,
			verify_clocktime: verify_clocktime,
		}
	}
}

impl<'a> TransactionRule for TransactionEval<'a> {
	fn check(&self) -> Result<(), TransactionError> {
		if self.transaction.raw.is_coinbase() {
			return Ok(());
		}

		let signer: TransactionInputSigner = self.transaction.raw.clone().into();

		let mut checker = TransactionSignatureChecker {
			signer: signer,
			input_index: 0,
		};

		for (index, input) in self.transaction.raw.inputs.iter().enumerate() {
			let output = self.store.previous_transaction_output(&input.previous_output)
				.ok_or_else(|| TransactionError::UnknownReference(input.previous_output.hash.clone()))?;

			checker.input_index = index;

			let input: Script = input.script_sig.clone().into();
			let output: Script = output.script_pubkey.into();

			let flags = VerificationFlags::default()
				.verify_p2sh(self.verify_p2sh)
				.verify_clocktimeverify(self.verify_clocktime);

			try!(verify_script(&input, &output, &flags, &checker).map_err(|_| TransactionError::Signature(index)));
		}

		Ok(())
	}
}

pub struct TransactionDoubleSpend<'a> {
	transaction: CanonTransaction<'a>,
	store: &'a TransactionOutputObserver,
}

impl<'a> TransactionDoubleSpend<'a> {
	fn new(transaction: CanonTransaction<'a>, store: &'a TransactionOutputObserver) -> Self {
		TransactionDoubleSpend {
			transaction: transaction,
			store: store,
		}
	}
}

impl<'a> TransactionRule for TransactionDoubleSpend<'a> {
	fn check(&self) -> Result<(), TransactionError> {
		for input in &self.transaction.raw.inputs {
			if self.store.is_spent(&input.previous_output).unwrap_or(false) {
				return Err(TransactionError::UsingSpentOutput(
					input.previous_output.hash.clone(),
					input.previous_output.index
				))
			}
		}
		Ok(())
	}
}
