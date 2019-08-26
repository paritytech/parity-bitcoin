use std::collections::HashMap;
use parking_lot::RwLock;
use hash::H256;
use bytes::Bytes;
use chain::{IndexedTransaction, OutPoint, TransactionOutput};
use {TransactionMeta};

/// Should be used to obtain all transactions from canon chain and forks.
pub trait TransactionProvider {
	/// Returns true if store contains given transaction.
	fn contains_transaction(&self, hash: &H256) -> bool {
		self.transaction(hash).is_some()
	}

	/// Resolves transaction body bytes by transaction hash.
	fn transaction_bytes(&self, hash: &H256) -> Option<Bytes>;

	/// Resolves serialized transaction info by transaction hash.
	fn transaction(&self, hash: &H256) -> Option<IndexedTransaction>;
}

/// Should be used to get canon chain transaction outputs.
pub trait TransactionOutputProvider: Send + Sync {
	/// Returns transaction output.
	fn transaction_output(&self, outpoint: &OutPoint, transaction_index: usize) -> Option<TransactionOutput>;

	/// Returns true if we know that output is double spent.
	fn is_spent(&self, outpoint: &OutPoint) -> bool;
}

/// Transaction meta provider stores transaction meta information
pub trait TransactionMetaProvider: Send + Sync {
	/// Returns None if transactin with given hash does not exist
	/// Otherwise returns transaction meta object
	fn transaction_meta(&self, hash: &H256) -> Option<TransactionMeta>;
}

/// Transaction output provider that caches all read outputs.
///
/// Not intended for long-lasting life, because it never clears its internal
/// cache. The backing storage is considered readonly for the cache lifetime.
pub struct CachedTransactionOutputProvider<'a> {
	backend: &'a dyn TransactionOutputProvider,
	cached_outputs: RwLock<HashMap<OutPoint, Option<TransactionOutput>>>,
}

impl<'a> CachedTransactionOutputProvider<'a> {
	/// Create new cached tx output provider backed by passed provider.
	pub fn new(backend: &'a dyn TransactionOutputProvider) -> Self {
		CachedTransactionOutputProvider {
			backend,
			cached_outputs: RwLock::new(HashMap::new()),
		}
	}
}

impl<'a> TransactionOutputProvider for CachedTransactionOutputProvider<'a> {
	fn transaction_output(&self, outpoint: &OutPoint, transaction_index: usize) -> Option<TransactionOutput> {
		let cached_value = self.cached_outputs.read().get(outpoint).cloned();
		match cached_value {
			Some(cached_value) => cached_value,
			None => {
				let value_from_backend = self.backend.transaction_output(outpoint, transaction_index);
				self.cached_outputs.write().insert(outpoint.clone(), value_from_backend.clone());
				value_from_backend
			},
		}
	}

	fn is_spent(&self, outpoint: &OutPoint) -> bool {
		self.backend.is_spent(outpoint)
	}
}
