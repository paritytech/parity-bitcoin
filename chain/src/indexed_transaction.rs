use std::{cmp, io, borrow, fmt};
use hash::H256;
use ser::{Deserializable, Reader, Error as ReaderError};
use transaction::Transaction;
use read_and_hash::ReadAndHash;

#[derive(Default, Clone)]
pub struct IndexedTransaction {
	pub hash: H256,
	pub raw: Transaction,
}

impl fmt::Debug for IndexedTransaction {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.debug_struct("IndexedTransaction")
			.field("hash", &self.hash.reversed())
			.field("raw", &self.raw)
			.finish()
	}
}

impl<T> From<T> for IndexedTransaction where Transaction: From<T> {
	fn from(other: T) -> Self {
		let tx = Transaction::from(other);
		IndexedTransaction {
			hash: tx.hash(),
			raw: tx,
		}
	}
}

impl IndexedTransaction {
	pub fn new(hash: H256, transaction: Transaction) -> Self {
		IndexedTransaction {
			hash: hash,
			raw: transaction,
		}
	}
}

impl cmp::PartialEq for IndexedTransaction {
	fn eq(&self, other: &Self) -> bool {
		self.hash == other.hash
	}
}

impl Deserializable for IndexedTransaction {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let data = try!(reader.read_and_hash::<Transaction>());
		// TODO: use len
		let tx = IndexedTransaction {
			raw: data.data,
			hash: data.hash,
		};

		Ok(tx)
	}
}

pub struct IndexedTransactionsRef<'a, T> where T: 'a {
	pub transactions: &'a [T],
}

impl<'a, T> IndexedTransactionsRef<'a, T> where T: borrow::Borrow<IndexedTransaction> {
	pub fn new(transactions: &'a [T]) -> Self {
		IndexedTransactionsRef {
			transactions: transactions,
		}
	}
}
