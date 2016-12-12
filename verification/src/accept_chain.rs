use rayon::prelude::{IntoParallelRefIterator, IndexedParallelIterator, ParallelIterator};
use db::SharedStore;
use network::Magic;
use error::Error;
use canon::CanonBlock;
use accept_block::BlockAcceptor;
use accept_header::HeaderAcceptor;
use accept_transaction::TransactionAcceptor;
use duplex_store::DuplexTransactionOutputProvider;

pub struct ChainAcceptor<'a> {
	pub block: BlockAcceptor<'a>,
	pub header: HeaderAcceptor<'a>,
	pub transactions: Vec<TransactionAcceptor<'a>>,
}

impl<'a> ChainAcceptor<'a> {
	pub fn new(store: &'a SharedStore, network: Magic, block: CanonBlock<'a>, height: u32) -> Self {
		let prevouts = DuplexTransactionOutputProvider::new(store.as_previous_transaction_output_provider(), block.raw());
		ChainAcceptor {
			block: BlockAcceptor::new(store.as_previous_transaction_output_provider(), network, block, height),
			header: HeaderAcceptor::new(store.as_block_header_provider(), network, block.header(), height),
			transactions: block.transactions()
				.into_iter()
				.map(|tx| TransactionAcceptor::new(store.as_transaction_meta_provider(), prevouts, network, tx, block.hash(), height, block.header.raw.time))
				.collect(),
		}
	}

	pub fn check(&self) -> Result<(), Error> {
		try!(self.block.check());
		try!(self.header.check());
		try!(self.check_transactions_with_eval(true));
		Ok(())
	}

	/// backwards test compatibility
	/// TODO: get rid of this
	pub fn check_with_eval(&self, eval: bool) -> Result<(), Error> {
		try!(self.block.check());
		try!(self.header.check());
		try!(self.check_transactions_with_eval(eval));
		Ok(())
	}

	fn check_transactions_with_eval(&self, eval: bool) -> Result<(), Error> {
		self.transactions.par_iter()
			.enumerate()
			.fold(|| Ok(()), |result, (index, tx)| result.and_then(|_| tx.check_with_eval(eval).map_err(|err| Error::Transaction(index, err))))
			.reduce(|| Ok(()), |acc, check| acc.and(check))
	}
}
