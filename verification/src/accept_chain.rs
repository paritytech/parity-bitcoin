use rayon::prelude::{IntoParallelRefIterator, IndexedParallelIterator, ParallelIterator};
use storage::{
	DuplexTransactionOutputProvider, TransactionOutputProvider, TransactionMetaProvider,
	BlockHeaderProvider,
};
use network::ConsensusParams;
use error::Error;
use canon::CanonBlock;
use accept_block::BlockAcceptor;
use accept_header::HeaderAcceptor;
use accept_transaction::TransactionAcceptor;
use deployments::BlockDeployments;
use VerificationLevel;

pub struct ChainAcceptor<'a> {
	pub block: BlockAcceptor<'a>,
	pub header: HeaderAcceptor<'a>,
	pub transactions: Vec<TransactionAcceptor<'a>>,
}

impl<'a> ChainAcceptor<'a> {
	pub fn new(
		tx_out_provider: &'a dyn TransactionOutputProvider,
		tx_meta_provider: &'a dyn TransactionMetaProvider,
		header_provider: &'a dyn BlockHeaderProvider,
		consensus: &'a ConsensusParams,
		verification_level: VerificationLevel,
		block: CanonBlock<'a>,
		height: u32,
		median_time_past: u32,
		deployments: &'a BlockDeployments,
	) -> Self {
		trace!(target: "verification", "Block verification {}", block.hash().to_reversed_str());
		let output_store = DuplexTransactionOutputProvider::new(tx_out_provider, block.raw());

		ChainAcceptor {
			block: BlockAcceptor::new(
				tx_out_provider,
				consensus,
				block,
				height,
				median_time_past,
				deployments,
				header_provider,
			),
			header: HeaderAcceptor::new(header_provider, consensus, block.header(), height, deployments),
			transactions: block.transactions()
				.into_iter()
				.enumerate()
				.map(|(tx_index, tx)| TransactionAcceptor::new(
						tx_meta_provider,
						output_store,
						consensus,
						tx,
						verification_level,
						block.hash(),
						height,
						block.header.raw.time,
						median_time_past,
						tx_index,
						deployments,
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
