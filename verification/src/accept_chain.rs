use rayon::prelude::{IntoParallelRefIterator, IndexedParallelIterator, ParallelIterator};
use db::Store;
use network::Magic;
use error::Error;
use canon::CanonBlock;
use accept_block::BlockAcceptor;
use accept_header::HeaderAcceptor;
use accept_transaction::TransactionAcceptor;
use duplex_store::{DuplexTransactionOutputProvider, DuplexTransactionOutputObserver};

pub struct ChainAcceptor<'a> {
	pub block: BlockAcceptor<'a>,
	pub header: HeaderAcceptor<'a>,
	pub transactions: Vec<TransactionAcceptor<'a>>,
}

impl<'a> ChainAcceptor<'a> {
	pub fn new(store: &'a Store, network: Magic, block: CanonBlock<'a>, height: u32) -> Self {
		trace!(target: "verification", "Block verification {}", block.hash().to_reversed_str());
		let prevouts = DuplexTransactionOutputProvider::new(store.as_previous_transaction_output_provider(), block.raw());
		let spents = DuplexTransactionOutputObserver::new(store.as_transaction_output_observer(), block.raw());
		ChainAcceptor {
			block: BlockAcceptor::new(store.as_previous_transaction_output_provider(), network, block, height),
			header: HeaderAcceptor::new(store.as_block_header_provider(), network, block.header(), height),
			transactions: block.transactions()
				.into_iter()
				.enumerate()
				.map(|(tx_index, tx)| TransactionAcceptor::new(
						store.as_transaction_meta_provider(),
						prevouts,
						spents,
						network,
						tx,
						block.hash(),
						height,
						block.header.raw.time,
						tx_index
				))
				.collect(),
		}
	}

	pub fn check(&self) -> Result<(), Error> {
		try!(self.block.check());
		try!(self.header.check());
		try!(self.check_transactions());
		Ok(())
	}

	fn check_transactions(&self) -> Result<(), Error> {
		self.transactions.par_iter()
			.enumerate()
			.fold(|| Ok(()), |result, (index, tx)| result.and_then(|_| tx.check().map_err(|err| Error::Transaction(index, err))))
			.reduce(|| Ok(()), |acc, check| acc.and(check))
	}
}
