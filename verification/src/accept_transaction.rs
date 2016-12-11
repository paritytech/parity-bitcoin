use canon::CanonTransaction;
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

