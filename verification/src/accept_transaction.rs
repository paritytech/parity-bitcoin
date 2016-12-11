use db::IndexedTransaction;
use error::TransactionError;

pub struct TransactionAcceptor<'a> {
	_tmp: CanonTransaction<'a>,
}

impl<'a> TransactionAcceptor<'a> {
	pub fn new(transaction: CanonTransaction<'a>) -> Self {
		TransactionAcceptor {
			_tmp: transaction,
		}
	}

	pub fn check(&self) -> Result<(), TransactionError> {
		Ok(())
	}
}

#[derive(Clone, Copy)]
pub struct CanonTransaction<'a> {
	transaction: &'a IndexedTransaction,
}

impl<'a> CanonTransaction<'a> {
	pub fn new(transaction: &'a IndexedTransaction) -> Self {
		CanonTransaction {
			transaction: transaction,
		}
	}
}
