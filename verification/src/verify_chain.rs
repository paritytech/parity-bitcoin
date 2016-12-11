use scoped_pool::Pool;
use db::IndexedBlock;
use network::Magic;
use error::Error;
use verify_block::BlockVerifier;
use verify_header::HeaderVerifier;
use verify_transaction::TransactionVerifier;

pub struct ChainVerifier<'a> {
	pub block: BlockVerifier<'a>,
	pub header: HeaderVerifier<'a>,
	pub transactions: Vec<TransactionVerifier<'a>>,
}

impl<'a> ChainVerifier<'a> {
	pub fn new(block: &'a IndexedBlock, network: Magic, current_time: u32) -> Self {
		ChainVerifier {
			block: BlockVerifier::new(block),
			header: HeaderVerifier::new(&block.header, network, current_time),
			transactions: block.transactions.iter().map(TransactionVerifier::new).collect(),
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
