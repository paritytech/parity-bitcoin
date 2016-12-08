use chain;

use indexed_block::IndexedBlock;
use transaction_meta::TransactionMeta;

struct TransactionData {
	// parent transaction for each input
	parents: Vec<chain::Transaction>,
	// meta of parent transaction for each input
	meta: Vec<TransactionMeta>,
}

impl TransactionData {

	fn collect(transaction: &chain::Tranaction) -> TransactionData {

	}

}

struct ExpandedBlock {
	block: IndexedBlock,
	// guaranteed to be the same length as block.transactions()
	transactions_data: Vec<TransactionData>,
}

impl ExpandedBlock {
	fn new(block: IndexedBlock) -> Self {
		ExpandedBlock {
			block: block,
			transactions_data: HashMap::new(),
		}
	}
}

