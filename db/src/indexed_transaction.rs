use std::{cmp, io};
use primitives::hash::H256;
use chain::{Transaction, OutPoint, TransactionOutput};
use serialization::{Deserializable, Reader, Error as ReaderError};
use read_and_hash::ReadAndHash;
use PreviousTransactionOutputProvider;

#[derive(Debug, Clone)]
pub struct IndexedTransaction {
	pub hash: H256,
	pub raw: Transaction,
}

impl From<Transaction> for IndexedTransaction {
	fn from(tx: Transaction) -> Self {
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

impl<'a> PreviousTransactionOutputProvider for &'a [IndexedTransaction] {
	fn previous_transaction_output(&self, prevout: &OutPoint) -> Option<TransactionOutput> {
		self.iter()
			.find(|tx| tx.hash == prevout.hash)
			.and_then(|tx| tx.raw.outputs.get(prevout.index as usize))
			.cloned()
	}
}
