use std::sync::Arc;
use chain::BlockHeader;
use {
	BestBlock, BlockProvider, BlockHeaderProvider, TransactionProvider, TransactionMetaProvider,
	TransactionOutputProvider, BlockChain, IndexedBlockProvider, Forkable, Error
};

pub trait CanonStore: Store + Forkable + ConfigStore {
	fn as_store(&self) -> &Store;
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
	fn best_header(&self) -> BlockHeader;

	/// get blockchain difficulty
	fn difficulty(&self) -> f64;
}

/// Allows casting Arc<Store> to reference to any substore type
pub trait AsSubstore: BlockChain + IndexedBlockProvider + TransactionProvider + TransactionMetaProvider + TransactionOutputProvider {
	fn as_block_provider(&self) -> &BlockProvider;

	fn as_block_header_provider(&self) -> &BlockHeaderProvider;

	fn as_transaction_provider(&self) -> &TransactionProvider;

	fn as_transaction_output_provider(&self) -> &TransactionOutputProvider;

	fn as_transaction_meta_provider(&self) -> &TransactionMetaProvider;
}

impl<T> AsSubstore for T where T: BlockChain + IndexedBlockProvider + TransactionProvider + TransactionMetaProvider + TransactionOutputProvider {
	fn as_block_provider(&self) -> &BlockProvider {
		&*self
	}

	fn as_block_header_provider(&self) -> &BlockHeaderProvider {
		&*self
	}

	fn as_transaction_provider(&self) -> &TransactionProvider {
		&*self
	}

	fn as_transaction_output_provider(&self) -> &TransactionOutputProvider {
		&*self
	}

	fn as_transaction_meta_provider(&self) -> &TransactionMetaProvider {
		&*self
	}
}

pub type SharedStore = Arc<CanonStore + Send + Sync>;
