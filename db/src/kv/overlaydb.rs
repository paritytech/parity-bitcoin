use parking_lot::Mutex;
use kv::{Transaction, Value, KeyValueDatabase, MemoryDatabase, KeyState, Key};

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

	fn get(&self, key: &Key) -> Result<KeyState<Value>, String> {
		match self.overlay.get(key)? {
			KeyState::Unknown => self.db.get(key),
			exists => Ok(exists)
		}
	}
}

pub struct AutoFlushingOverlayDatabase<T> where T: KeyValueDatabase {
	db: T,
	overlay: MemoryDatabase,
	operations: Mutex<usize>,
	max_operations: usize,
}

impl<T> AutoFlushingOverlayDatabase<T> where T: KeyValueDatabase {
	pub fn new(db: T, max_operations: usize) -> Self {
		AutoFlushingOverlayDatabase {
			db: db,
			overlay: MemoryDatabase::default(),
			operations: Mutex::default(),
			max_operations: max_operations,
		}
	}

	fn flush(&self) -> Result<(), String> {
		self.db.write(self.overlay.drain_transaction())
	}
}

impl<T> KeyValueDatabase for AutoFlushingOverlayDatabase<T> where T: KeyValueDatabase {
	fn write(&self, tx: Transaction) -> Result<(), String> {
		let mut operations = self.operations.lock();
		*operations += 1;
		self.overlay.write(tx)?;
		if *operations == self.max_operations {
			self.flush()?;
			*operations = 0;
		}
		Ok(())
	}

	fn get(&self, key: &Key) -> Result<KeyState<Value>, String> {
		match self.overlay.get(key)? {
			KeyState::Unknown => self.db.get(key),
			exists => Ok(exists)
		}
	}
}

impl<T> Drop for AutoFlushingOverlayDatabase<T> where T: KeyValueDatabase {
	fn drop(&mut self) {
		self.flush().expect("Failed to save database");
	}
}
