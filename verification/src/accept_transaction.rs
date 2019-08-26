use primitives::hash::H256;
use primitives::bytes::Bytes;
use ser::Serializable;
use storage::{TransactionMetaProvider, TransactionOutputProvider, DuplexTransactionOutputProvider,
	transaction_index_for_output_check};
use network::{ConsensusParams, ConsensusFork};
use script::{Script, verify_script, VerificationFlags, TransactionSignatureChecker, TransactionInputSigner, SignatureVersion};
use deployments::BlockDeployments;
use script::Builder;
use sigops::transaction_sigops;
use canon::CanonTransaction;
use constants::{COINBASE_MATURITY};
use error::TransactionError;
use VerificationLevel;

pub struct TransactionAcceptor<'a> {
	pub size: TransactionSize<'a>,
	pub premature_witness: TransactionPrematureWitness<'a>,
	pub bip30: TransactionBip30<'a>,
	pub missing_inputs: TransactionMissingInputs<'a>,
	pub maturity: TransactionMaturity<'a>,
	pub overspent: TransactionOverspent<'a>,
	pub double_spent: TransactionDoubleSpend<'a>,
	pub return_replay_protection: TransactionReturnReplayProtection<'a>,
	pub eval: TransactionEval<'a>,
}

impl<'a> TransactionAcceptor<'a> {
	pub fn new(
		// in case of block validation, it's only current block,
		meta_store: &'a dyn TransactionMetaProvider,
		// previous transaction outputs
		// in case of block validation, that's database and currently processed block
		output_store: DuplexTransactionOutputProvider<'a>,
		consensus: &'a ConsensusParams,
		transaction: CanonTransaction<'a>,
		verification_level: VerificationLevel,
		block_hash: &'a H256,
		height: u32,
		time: u32,
		median_time_past: u32,
		transaction_index: usize,
		deployments: &'a BlockDeployments<'a>,
	) -> Self {
		trace!(target: "verification", "Tx verification {}", transaction.hash.to_reversed_str());
		let tx_ordering = consensus.fork.transaction_ordering(median_time_past);
		let missing_input_tx_index = transaction_index_for_output_check(tx_ordering,transaction_index);
		TransactionAcceptor {
			size: TransactionSize::new(transaction, consensus, median_time_past),
			premature_witness: TransactionPrematureWitness::new(transaction, deployments),
			bip30: TransactionBip30::new_for_sync(transaction, meta_store, consensus, block_hash, height),
			missing_inputs: TransactionMissingInputs::new(transaction, output_store, missing_input_tx_index),
			maturity: TransactionMaturity::new(transaction, meta_store, height),
			overspent: TransactionOverspent::new(transaction, output_store),
			double_spent: TransactionDoubleSpend::new(transaction, output_store),
			return_replay_protection: TransactionReturnReplayProtection::new(transaction, consensus, height),
			eval: TransactionEval::new(transaction, output_store, consensus, verification_level, height, time, median_time_past, deployments),
		}
	}

	pub fn check(&self) -> Result<(), TransactionError> {
		try!(self.size.check());
		try!(self.premature_witness.check());
		try!(self.bip30.check());
		try!(self.missing_inputs.check());
		try!(self.maturity.check());
		try!(self.overspent.check());
		try!(self.double_spent.check());
		try!(self.return_replay_protection.check());
		try!(self.eval.check());
		Ok(())
	}
}

pub struct MemoryPoolTransactionAcceptor<'a> {
	pub size: TransactionSize<'a>,
	pub missing_inputs: TransactionMissingInputs<'a>,
	pub maturity: TransactionMaturity<'a>,
	pub overspent: TransactionOverspent<'a>,
	pub sigops: TransactionSigops<'a>,
	pub double_spent: TransactionDoubleSpend<'a>,
	pub return_replay_protection: TransactionReturnReplayProtection<'a>,
	pub eval: TransactionEval<'a>,
}

impl<'a> MemoryPoolTransactionAcceptor<'a> {
	pub fn new(
		// TODO: in case of memory pool it should be db and memory pool
		meta_store: &'a dyn TransactionMetaProvider,
		// in case of memory pool it should be db and memory pool
		output_store: DuplexTransactionOutputProvider<'a>,
		consensus: &'a ConsensusParams,
		transaction: CanonTransaction<'a>,
		height: u32,
		time: u32,
		median_time_past: u32,
		deployments: &'a BlockDeployments<'a>,
	) -> Self {
		trace!(target: "verification", "Mempool-Tx verification {}", transaction.hash.to_reversed_str());
		let transaction_index = 0;
		let max_block_sigops = consensus.fork.max_block_sigops(height, consensus.fork.max_block_size(height, median_time_past));
		MemoryPoolTransactionAcceptor {
			size: TransactionSize::new(transaction, consensus, median_time_past),
			missing_inputs: TransactionMissingInputs::new(transaction, output_store, transaction_index),
			maturity: TransactionMaturity::new(transaction, meta_store, height),
			overspent: TransactionOverspent::new(transaction, output_store),
			sigops: TransactionSigops::new(transaction, output_store, consensus, max_block_sigops, time),
			double_spent: TransactionDoubleSpend::new(transaction, output_store),
			return_replay_protection: TransactionReturnReplayProtection::new(transaction, consensus, height),
			eval: TransactionEval::new(transaction, output_store, consensus, VerificationLevel::Full, height, time, median_time_past, deployments),
		}
	}

	pub fn check(&self) -> Result<(), TransactionError> {
		// Bip30 is not checked because we don't need to allow tx pool acceptance of an unspent duplicate.
		// Tx pool validation is not strinctly a matter of consensus.
		try!(self.size.check());
		try!(self.missing_inputs.check());
		try!(self.maturity.check());
		try!(self.overspent.check());
		try!(self.sigops.check());
		try!(self.double_spent.check());
		try!(self.return_replay_protection.check());
		try!(self.eval.check());
		Ok(())
	}
}

/// Bip30 validation
///
/// A transaction hash that exists in the chain is not acceptable even if
/// the original is spent in the new block. This is not necessary nor is it
/// described by BIP30, but it is in the code referenced by BIP30. As such
/// the tx pool need only test against the chain, skipping the pool.
///
/// source:
/// https://github.com/libbitcoin/libbitcoin/blob/61759b2fd66041bcdbc124b2f04ed5ddc20c7312/src/chain/transaction.cpp#L780-L785
pub struct TransactionBip30<'a> {
	transaction: CanonTransaction<'a>,
	store: &'a dyn TransactionMetaProvider,
	exception: bool,
}

impl<'a> TransactionBip30<'a> {
	fn new_for_sync(
		transaction: CanonTransaction<'a>,
		store: &'a dyn TransactionMetaProvider,
		consensus_params: &'a ConsensusParams,
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

	fn check(&self) -> Result<(), TransactionError> {
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
	transaction_index: usize,
}

impl<'a> TransactionMissingInputs<'a> {
	fn new(transaction: CanonTransaction<'a>, store: DuplexTransactionOutputProvider<'a>, transaction_index: usize) -> Self {
		TransactionMissingInputs {
			transaction: transaction,
			store: store,
			transaction_index: transaction_index,
		}
	}

	fn check(&self) -> Result<(), TransactionError> {
		let missing_index = self.transaction.raw.inputs.iter()
			.position(|input| {
				let is_not_null = !input.previous_output.is_null();
				let is_missing = self.store.transaction_output(&input.previous_output, self.transaction_index).is_none();
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
	store: &'a dyn TransactionMetaProvider,
	height: u32,
}

impl<'a> TransactionMaturity<'a> {
	fn new(transaction: CanonTransaction<'a>, store: &'a dyn TransactionMetaProvider, height: u32) -> Self {
		TransactionMaturity {
			transaction: transaction,
			store: store,
			height: height,
		}
	}

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

	fn check(&self) -> Result<(), TransactionError> {
		if self.transaction.raw.is_coinbase() {
			return Ok(());
		}

		let available = self.transaction.raw.inputs.iter()
			.map(|input| self.store.transaction_output(&input.previous_output, usize::max_value()).map(|o| o.value).unwrap_or(0))
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
	consensus_params: &'a ConsensusParams,
	max_sigops: usize,
	time: u32,
}

impl<'a> TransactionSigops<'a> {
	fn new(transaction: CanonTransaction<'a>, store: DuplexTransactionOutputProvider<'a>, consensus_params: &'a ConsensusParams, max_sigops: usize, time: u32) -> Self {
		TransactionSigops {
			transaction: transaction,
			store: store,
			consensus_params: consensus_params,
			max_sigops: max_sigops,
			time: time,
		}
	}

	fn check(&self) -> Result<(), TransactionError> {
		let bip16_active = self.time >= self.consensus_params.bip16_time;
		let checkdatasig_active = match self.consensus_params.fork {
			ConsensusFork::BitcoinCash(ref fork) => self.time >= fork.magnetic_anomaly_time,
			_ => false
		};
		let sigops = transaction_sigops(&self.transaction.raw, &self.store, bip16_active, checkdatasig_active);
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
	verification_level: VerificationLevel,
	verify_p2sh: bool,
	verify_strictenc: bool,
	verify_locktime: bool,
	verify_checksequence: bool,
	verify_dersig: bool,
	verify_witness: bool,
	verify_nulldummy: bool,
	verify_monolith_opcodes: bool,
	verify_magnetic_anomaly_opcodes: bool,
	verify_sigpushonly: bool,
	verify_cleanstack: bool,
	signature_version: SignatureVersion,
}

impl<'a> TransactionEval<'a> {
	fn new(
		transaction: CanonTransaction<'a>,
		store: DuplexTransactionOutputProvider<'a>,
		params: &ConsensusParams,
		verification_level: VerificationLevel,
		height: u32,
		time: u32,
		median_timestamp: u32,
		deployments: &'a BlockDeployments,
	) -> Self {
		let verify_p2sh = time >= params.bip16_time;
		let verify_strictenc = match params.fork {
			ConsensusFork::BitcoinCash(ref fork) if height >= fork.height => true,
			_ => false,
		};
		let verify_locktime = height >= params.bip65_height;
		let verify_dersig = height >= params.bip66_height;
		let verify_monolith_opcodes = match params.fork {
			ConsensusFork::BitcoinCash(ref fork) => median_timestamp >= fork.monolith_time,
			_ => false,
		};
		let verify_magnetic_anomaly_opcodes = match params.fork {
			ConsensusFork::BitcoinCash(ref fork) => median_timestamp >= fork.magnetic_anomaly_time,
			_ => false,
		};
		let signature_version = match params.fork {
			ConsensusFork::BitcoinCash(ref fork) if height >= fork.height => SignatureVersion::ForkId,
			ConsensusFork::BitcoinCore | ConsensusFork::BitcoinCash(_) => SignatureVersion::Base,
		};

		let verify_checksequence = deployments.csv();
		let verify_witness = deployments.segwit();
		let verify_nulldummy = verify_witness;
		let verify_sigpushonly = verify_magnetic_anomaly_opcodes;
		let verify_cleanstack = verify_magnetic_anomaly_opcodes;

		TransactionEval {
			transaction: transaction,
			store: store,
			verification_level: verification_level,
			verify_p2sh: verify_p2sh,
			verify_strictenc: verify_strictenc,
			verify_locktime: verify_locktime,
			verify_checksequence: verify_checksequence,
			verify_dersig: verify_dersig,
			verify_witness: verify_witness,
			verify_nulldummy: verify_nulldummy,
			verify_monolith_opcodes: verify_monolith_opcodes,
			verify_magnetic_anomaly_opcodes: verify_magnetic_anomaly_opcodes,
			verify_sigpushonly: verify_sigpushonly,
			verify_cleanstack: verify_cleanstack,
			signature_version: signature_version,
		}
	}

	fn check(&self) -> Result<(), TransactionError> {
		if self.verification_level == VerificationLevel::Header
			|| self.verification_level == VerificationLevel::NoVerification {
			return Ok(());
		}

		if self.transaction.raw.is_coinbase() {
			return Ok(());
		}

		let signer: TransactionInputSigner = self.transaction.raw.clone().into();

		let mut checker = TransactionSignatureChecker {
			signer: signer,
			input_index: 0,
			input_amount: 0,
		};

		for (index, input) in self.transaction.raw.inputs.iter().enumerate() {
			let output = self.store.transaction_output(&input.previous_output, usize::max_value())
				.ok_or_else(|| TransactionError::UnknownReference(input.previous_output.hash.clone()))?;

			checker.input_index = index;
			checker.input_amount = output.value;

			let script_witness = &input.script_witness;
			let input: Script = input.script_sig.clone().into();
			let output: Script = output.script_pubkey.into();

			let flags = VerificationFlags::default()
				.verify_p2sh(self.verify_p2sh)
				.verify_strictenc(self.verify_strictenc)
				.verify_locktime(self.verify_locktime)
				.verify_checksequence(self.verify_checksequence)
				.verify_dersig(self.verify_dersig)
				.verify_nulldummy(self.verify_nulldummy)
				.verify_witness(self.verify_witness)
				.verify_concat(self.verify_monolith_opcodes)
				.verify_split(self.verify_monolith_opcodes)
				.verify_and(self.verify_monolith_opcodes)
				.verify_or(self.verify_monolith_opcodes)
				.verify_xor(self.verify_monolith_opcodes)
				.verify_div(self.verify_monolith_opcodes)
				.verify_mod(self.verify_monolith_opcodes)
				.verify_bin2num(self.verify_monolith_opcodes)
				.verify_num2bin(self.verify_monolith_opcodes)
				.verify_checkdatasig(self.verify_magnetic_anomaly_opcodes)
				.verify_sigpushonly(self.verify_sigpushonly)
				.verify_cleanstack(self.verify_cleanstack);

			try!(verify_script(&input, &output, &script_witness, &flags, &checker, self.signature_version)
				.map_err(|e| TransactionError::Signature(index, e)));
		}

		Ok(())
	}
}

pub struct TransactionDoubleSpend<'a> {
	transaction: CanonTransaction<'a>,
	store: DuplexTransactionOutputProvider<'a>,
}

impl<'a> TransactionDoubleSpend<'a> {
	fn new(transaction: CanonTransaction<'a>, store: DuplexTransactionOutputProvider<'a>) -> Self {
		TransactionDoubleSpend {
			transaction: transaction,
			store: store,
		}
	}

	fn check(&self) -> Result<(), TransactionError> {
		if self.transaction.raw.is_coinbase() {
			return Ok(());
		}

		for input in &self.transaction.raw.inputs {
			if self.store.is_spent(&input.previous_output) {
				return Err(TransactionError::UsingSpentOutput(
					input.previous_output.hash.clone(),
					input.previous_output.index
				))
			}
		}
		Ok(())
	}
}

pub struct TransactionReturnReplayProtection<'a> {
	transaction: CanonTransaction<'a>,
	consensus: &'a ConsensusParams,
	height: u32,
}

lazy_static! {
	pub static ref BITCOIN_CASH_RETURN_REPLAY_PROTECTION_SCRIPT: Bytes = Builder::default()
		.return_bytes(b"Bitcoin: A Peer-to-Peer Electronic Cash System")
		.into_bytes();
}

impl<'a> TransactionReturnReplayProtection<'a> {
	fn new(transaction: CanonTransaction<'a>, consensus: &'a ConsensusParams, height: u32) -> Self {
		TransactionReturnReplayProtection {
			transaction: transaction,
			consensus: consensus,
			height: height,
		}
	}

	fn check(&self) -> Result<(), TransactionError> {
		if let ConsensusFork::BitcoinCash(ref fork) = self.consensus.fork {
			// Transactions with such OP_RETURNs shall be considered valid again for block 530,001 and onwards
			if self.height >= fork.height && self.height <= 530_000 {
				if (*self.transaction).raw.outputs.iter()
					.any(|out| out.script_pubkey == *BITCOIN_CASH_RETURN_REPLAY_PROTECTION_SCRIPT) {
					return Err(TransactionError::ReturnReplayProtection)
				}
			}
		}

		Ok(())
	}
}

pub struct TransactionPrematureWitness<'a> {
	transaction: CanonTransaction<'a>,
	segwit_active: bool,
}

impl<'a> TransactionPrematureWitness<'a> {
	fn new(transaction: CanonTransaction<'a>, deployments: &'a BlockDeployments<'a>) -> Self {
		let segwit_active = deployments.segwit();

		TransactionPrematureWitness {
			transaction: transaction,
			segwit_active: segwit_active,
		}
	}

	fn check(&self) -> Result<(), TransactionError> {
		if !self.segwit_active && (*self.transaction).raw.has_witness() {
			Err(TransactionError::PrematureWitness)
		} else {
			Ok(())
		}
	}
}

pub struct TransactionSize<'a> {
	transaction: CanonTransaction<'a>,
	min_transaction_size: usize,
}

impl<'a> TransactionSize<'a> {
	fn new(transaction: CanonTransaction<'a>, consensus: &'a ConsensusParams, median_time_past: u32) -> Self {
		let min_transaction_size = consensus.fork.min_transaction_size(median_time_past);
		TransactionSize {
			transaction: transaction,
			min_transaction_size,
		}
	}

	fn check(&self) -> Result<(), TransactionError> {
		if self.min_transaction_size != 0 && self.transaction.raw.serialized_size() < self.min_transaction_size {
			Err(TransactionError::MinSize)
		} else {
			Ok(())
		}
	}
}

#[cfg(test)]
mod tests {
	use chain::{IndexedTransaction, Transaction, TransactionOutput};
	use network::{Network, ConsensusParams, ConsensusFork, BitcoinCashConsensusParams};
	use script::Builder;
	use canon::CanonTransaction;
	use error::TransactionError;
	use super::{TransactionReturnReplayProtection, TransactionSize};

	#[test]
	fn return_replay_protection_works() {
		let transaction: IndexedTransaction = Transaction {
			version: 1,
			inputs: vec![],
			outputs: vec![TransactionOutput {
				value: 0,
				script_pubkey: Builder::default()
					.return_bytes(b"Bitcoin: A Peer-to-Peer Electronic Cash System")
					.into_bytes(),
			}],
			lock_time: 0xffffffff,
		}.into();

		assert_eq!(transaction.raw.outputs[0].script_pubkey.len(), 46 + 2);

		let consensus = ConsensusParams::new(Network::Mainnet, ConsensusFork::BitcoinCash(BitcoinCashConsensusParams::new(Network::Mainnet)));
		let checker = TransactionReturnReplayProtection::new(CanonTransaction::new(&transaction), &consensus, consensus.fork.activation_height());
		assert_eq!(checker.check(), Err(TransactionError::ReturnReplayProtection));
		let checker = TransactionReturnReplayProtection::new(CanonTransaction::new(&transaction), &consensus, consensus.fork.activation_height() - 1);
		assert_eq!(checker.check(), Ok(()));

		let consensus = ConsensusParams::new(Network::Mainnet, ConsensusFork::BitcoinCore);
		let checker = TransactionReturnReplayProtection::new(CanonTransaction::new(&transaction), &consensus, 100);
		assert_eq!(checker.check(), Ok(()));
	}

	#[test]
	fn transaction_size_works() {
		let small_tx = Transaction::default();
		let big_tx: Transaction = "01000000000102fff7f7881a8099afa6940d42d1e7f6362bec38171ea3edf433541db4e4ad969f00000000494830450221008b9d1dc26ba6a9cb62127b02742fa9d754cd3bebf337f7a55d114c8e5cdd30be022040529b194ba3f9281a99f2b1c0a19c0489bc22ede944ccf4ecbab4cc618ef3ed01eeffffffef51e1b804cc89d182d279655c3aa89e815b1b309fe287d9b2b55d57b90ec68a0100000000ffffffff02202cb206000000001976a9148280b37df378db99f66f85c95a783a76ac7a6d5988ac9093510d000000001976a9143bde42dbee7e4dbe6a21b2d50ce2f0167faa815988ac000247304402203609e17b84f6a7d30c80bfa610b5b4542f32a8a0d5447a12fb1366d7f01cc44a0220573a954c4518331561406f90300e8f3358f51928d43c212a8caed02de67eebee0121025476c2e83188368da1ff3e292e7acafcdb3566bb0ad253f62fc70f07aeee635711000000".into();
		let small_tx = IndexedTransaction::new(small_tx.hash(), small_tx);
		let big_tx = IndexedTransaction::new(big_tx.hash(), big_tx);
		let small_tx = CanonTransaction::new(&small_tx);
		let big_tx = CanonTransaction::new(&big_tx);

		let unrestricted_consensus = ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore);
		let restricted_consensus = ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCash(BitcoinCashConsensusParams::new(Network::Unitest)));

		// no restrictions
		let checker = TransactionSize::new(small_tx, &unrestricted_consensus, 10000000);
		assert_eq!(checker.check(), Ok(()));

		// big + restricted
		let checker = TransactionSize::new(big_tx, &restricted_consensus, 2000000000);
		assert_eq!(checker.check(), Ok(()));

		// small + restricted
		let checker = TransactionSize::new(small_tx, &restricted_consensus, 2000000000);
		assert_eq!(checker.check(), Err(TransactionError::MinSize));
	}
}
