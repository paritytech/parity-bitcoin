use std::sync::Arc;
use chain::IndexedBlockHeader;
use {
	BestBlock, BlockProvider, BlockHeaderProvider, TransactionProvider, TransactionMetaProvider,
	TransactionOutputProvider, BlockChain, Forkable, Error,
};

pub trait CanonStore: Store + Forkable + ConfigStore {
	fn as_store(&self) -> &dyn Store;
}

/// Configuration storage interface
pub trait ConfigStore {
	/// get consensus_fork this database is configured for
	fn consensus_fork(&self) -> Result<Option<String>, Error>;

	/// set consensus_fork this database is configured for
	fn set_consensus_fork(&self, consensus_fork: &str) -> Result<(), Error>;
}

/// Blockchain storage interface
pub trait Store: AsSubstore {
	/// get best block
	fn best_block(&self) -> BestBlock;

	/// get best header
	fn best_header(&self) -> IndexedBlockHeader;

	/// get blockchain difficulty
	fn difficulty(&self) -> f64;
}

/// Allows casting Arc<Store> to reference to any substore type
pub trait AsSubstore: BlockChain + BlockProvider + TransactionProvider + TransactionMetaProvider + TransactionOutputProvider {
	fn as_block_provider(&self) -> &dyn BlockProvider;

	fn as_block_header_provider(&self) -> &dyn BlockHeaderProvider;

	fn as_transaction_provider(&self) -> &dyn TransactionProvider;

	fn as_transaction_output_provider(&self) -> &dyn TransactionOutputProvider;

	fn as_transaction_meta_provider(&self) -> &dyn TransactionMetaProvider;
}

impl<T> AsSubstore for T where T: BlockChain + BlockProvider + TransactionProvider + TransactionMetaProvider + TransactionOutputProvider {
	fn as_block_provider(&self) -> &dyn BlockProvider {
		&*self
	}

	fn as_block_header_provider(&self) -> &dyn BlockHeaderProvider {
		&*self
	}

	fn as_transaction_provider(&self) -> &dyn TransactionProvider {
		&*self
	}

	fn as_transaction_output_provider(&self) -> &dyn TransactionOutputProvider {
		&*self
	}

	fn as_transaction_meta_provider(&self) -> &dyn TransactionMetaProvider {
		&*self
	}
}

pub type SharedStore = Arc<dyn CanonStore + Send + Sync>;
