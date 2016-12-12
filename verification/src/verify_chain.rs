use rayon::prelude::{IntoParallelRefIterator, IndexedParallelIterator, ParallelIterator};
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
		try!(self.check_transactions());
		Ok(())
	}

	/// backwards test compatibility
	/// TODO: get rid of this
	pub fn check_with_pow(&self, pow: bool) -> Result<(), Error> {
		try!(self.block.check());
		try!(self.header.check_with_pow(pow));
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
