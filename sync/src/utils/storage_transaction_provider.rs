use chain::{TransactionOutput, OutPoint};
use db::{PreviousTransactionOutputProvider, TransactionOutputObserver};
use super::super::types::StorageRef;

/// Transaction output observer, which looks into storage
pub struct StorageTransactionOutputProvider {
	/// Storage reference
	storage: StorageRef,
}

impl StorageTransactionOutputProvider {
	pub fn with_storage(storage: StorageRef) -> Self {
		StorageTransactionOutputProvider {
			storage: storage,
		}
	}
}

impl TransactionOutputObserver for StorageTransactionOutputProvider {
	fn is_spent(&self, prevout: &OutPoint) -> Option<bool> {
		self.storage
			.transaction_meta(&prevout.hash)
			.and_then(|tm| tm.is_spent(prevout.index as usize))
	}
}

impl PreviousTransactionOutputProvider for StorageTransactionOutputProvider {
	fn previous_transaction_output(&self, prevout: &OutPoint) -> Option<TransactionOutput> {
		self.storage.as_previous_transaction_output_provider().previous_transaction_output(prevout)
	}
}
