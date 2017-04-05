use kv::{Transaction, Location, Value, KeyValueDatabase, MemoryDatabase};

pub struct OverlayDatabase<'a, T> where T: 'a + KeyValueDatabase {
	db: &'a T,
	overlay: MemoryDatabase,
}

impl<'a, T> OverlayDatabase<'a, T> where T: 'a + KeyValueDatabase {
	pub fn new(db: &'a T) -> Self {
		OverlayDatabase {
			db: db,
			overlay: MemoryDatabase::default(),
		}
	}

	pub fn flush(&self) -> Result<(), String> {
		self.db.write(self.overlay.drain_transaction())
	}
}

impl<'a, T> KeyValueDatabase for OverlayDatabase<'a, T> where T: 'a + KeyValueDatabase {
	fn write(&self, tx: Transaction) -> Result<(), String> {
		self.overlay.write(tx)
	}

	fn get(&self, location: Location, key: &[u8]) -> Result<Option<Value>, String> {
		if self.overlay.is_known(location, key) {
			self.overlay.get(location, key)
		} else {
			self.db.get(location, key)
		}
	}
}
