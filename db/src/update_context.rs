use kvdb::{DBTransaction, Database};
use transaction_meta::TransactionMeta;
use std::collections::HashMap;
use storage::COL_TRANSACTIONS_META;
use primitives::hash::H256;
use error::Error;

pub struct UpdateContext {
	pub meta: HashMap<H256, TransactionMeta>,
	pub db_transaction: DBTransaction,
	meta_snapshot: Option<HashMap<H256, TransactionMeta>>,
	target: Option<H256>,
}

impl UpdateContext {
	pub fn new(db: &Database) -> Self {
		UpdateContext {
			meta: HashMap::new(),
			db_transaction: db.transaction(),
			meta_snapshot: None,
			target: None,
		}
	}

	pub fn target(&mut self, hash: &H256) {
		self.target = Some(hash.clone());
		trace!("Initialized transaction for block {:?}", self.target);
	}

	pub fn apply(mut self, db: &Database) -> Result<(), Error> {
		// actually saving meta
		for (hash, meta) in self.meta.drain() {
			self.db_transaction.put(Some(COL_TRANSACTIONS_META), &*hash, &meta.into_bytes());
		}

		try!(db.write(self.db_transaction));

		if let Some(target) = self.target {
			trace!("Applied transaction for block {:?}", target);
		}
		Ok(())
	}

	pub fn restore_point(&mut self) {
		// todo: optimize clone here
		self.meta_snapshot = Some(self.meta.clone());
		self.db_transaction.remember();
	}

	pub fn restore(&mut self) {
		if let Some(meta_snapshot) = self.meta_snapshot.take() {
			self.meta = meta_snapshot;
			self.db_transaction.rollback();
		}
	}
}
