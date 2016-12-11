use scoped_pool::Pool;
use db::SharedStore;
use network::Magic;
use error::Error;
use accept_block::{CanonBlock, BlockAcceptor};
use accept_header::HeaderAcceptor;
use accept_transaction::TransactionAcceptor;

pub struct ChainAcceptor<'a> {
	pub block: BlockAcceptor<'a>,
	pub header: HeaderAcceptor<'a>,
	pub transactions: Vec<TransactionAcceptor<'a>>,
}

impl<'a> ChainAcceptor<'a> {
	pub fn new(store: &'a SharedStore, network: Magic, block: CanonBlock<'a>, height: u32) -> Self {
		ChainAcceptor {
			block: BlockAcceptor::new(store, network, block, height),
			header: HeaderAcceptor::new(block.header()),
			transactions: block.transactions().into_iter().map(TransactionAcceptor::new).collect(),
		}
	}

	pub fn check(&self) -> Result<(), Error> {
		try!(self.block.check());
		try!(self.header.check());
		self.transactions.iter()
			.enumerate()
			.map(|(index, tx)| tx.check().map_err(|err| Error::Transaction(index, err)))
			.collect::<Result<Vec<_>, _>>()?;
		Ok(())
	}

	pub fn parallel_check(&self, _pool: &Pool) -> Result<(), Error> {
		unimplemented!();
	}
}
