use db;
use chain;
use primitives::H256;
use std::collections::HashMap;

struct BlockTransactionLookup<'a, 'b> {
	store: &'a db::Store,
	block: &'b db::IndexedBlock,
	cache: HashMap<H256, chain::Transaction>,
}

impl<'a, 'b> BlockTransactionLookup<'a, 'b> {
	fn new(store: &'a db::Store, block: &'b db::IndexedBlock) -> BlockTransactionLookup<'a, 'b> {
		BlockTransactionLookup { store: store, block: block, cache: HashMap::new() }
	}

	fn find(&mut self, hash: &H256) -> Option<(&H256, &chain::Transaction)> {
		None
	}
}
