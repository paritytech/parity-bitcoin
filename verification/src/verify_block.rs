use std::collections::HashSet;
use chain::IndexedBlock;
use network::ConsensusFork;
use sigops::transaction_sigops;
use storage::NoopStore;
use error::{Error, TransactionError};

pub struct BlockVerifier<'a> {
	pub empty: BlockEmpty<'a>,
	pub coinbase: BlockCoinbase<'a>,
	pub serialized_size: BlockSerializedSize<'a>,
	pub extra_coinbases: BlockExtraCoinbases<'a>,
	pub transactions_uniqueness: BlockTransactionsUniqueness<'a>,
	pub sigops: BlockSigops<'a>,
	pub merkle_root: BlockMerkleRoot<'a>,
}

impl<'a> BlockVerifier<'a> {
	pub fn new(block: &'a IndexedBlock) -> Self {
		BlockVerifier {
			empty: BlockEmpty::new(block),
			coinbase: BlockCoinbase::new(block),
			serialized_size: BlockSerializedSize::new(block, ConsensusFork::absolute_maximum_block_size()),
			extra_coinbases: BlockExtraCoinbases::new(block),
			transactions_uniqueness: BlockTransactionsUniqueness::new(block),
			sigops: BlockSigops::new(block, ConsensusFork::absolute_maximum_block_sigops()),
			merkle_root: BlockMerkleRoot::new(block),
		}
	}

	pub fn check(&self) -> Result<(), Error> {
		try!(self.empty.check());
		try!(self.coinbase.check());
		try!(self.serialized_size.check());
		try!(self.extra_coinbases.check());
		try!(self.transactions_uniqueness.check());
		try!(self.sigops.check());
		try!(self.merkle_root.check());
		Ok(())
	}
}

pub struct BlockEmpty<'a> {
	block: &'a IndexedBlock,
}

impl<'a> BlockEmpty<'a> {
	fn new(block: &'a IndexedBlock) -> Self {
		BlockEmpty {
			block: block,
		}
	}

	fn check(&self) -> Result<(), Error> {
		if self.block.transactions.is_empty() {
			Err(Error::Empty)
		} else {
			Ok(())
		}
	}
}

pub struct BlockSerializedSize<'a> {
	block: &'a IndexedBlock,
	max_size: usize,
}

impl<'a> BlockSerializedSize<'a> {
	fn new(block: &'a IndexedBlock, max_size: usize) -> Self {
		BlockSerializedSize {
			block: block,
			max_size: max_size,
		}
	}

	fn check(&self) -> Result<(), Error> {
		let size = self.block.size();
		if size > self.max_size {
			Err(Error::Size(size))
		} else {
			Ok(())
		}
	}
}

pub struct BlockCoinbase<'a> {
	block: &'a IndexedBlock,
}

impl<'a> BlockCoinbase<'a> {
	fn new(block: &'a IndexedBlock) -> Self {
		BlockCoinbase {
			block: block,
		}
	}

	fn check(&self) -> Result<(), Error> {
		if self.block.transactions.first().map(|tx| tx.raw.is_coinbase()).unwrap_or(false) {
			Ok(())
		} else {
			Err(Error::Coinbase)
		}
	}
}

pub struct BlockExtraCoinbases<'a> {
	block: &'a IndexedBlock,
}

impl<'a> BlockExtraCoinbases<'a> {
	fn new(block: &'a IndexedBlock) -> Self {
		BlockExtraCoinbases {
			block: block,
		}
	}

	fn check(&self) -> Result<(), Error> {
		let misplaced = self.block.transactions.iter()
			.skip(1)
			.position(|tx| tx.raw.is_coinbase());

		match misplaced {
			Some(index) => Err(Error::Transaction(index + 1, TransactionError::MisplacedCoinbase)),
			None => Ok(()),
		}
	}
}

pub struct BlockTransactionsUniqueness<'a> {
	block: &'a IndexedBlock,
}

impl<'a> BlockTransactionsUniqueness<'a> {
	fn new(block: &'a IndexedBlock) -> Self {
		BlockTransactionsUniqueness {
			block: block,
		}
	}

	fn check(&self) -> Result<(), Error> {
		let hashes = self.block.transactions.iter().map(|tx| tx.hash.clone()).collect::<HashSet<_>>();
		if hashes.len() == self.block.transactions.len() {
			Ok(())
		} else {
			Err(Error::DuplicatedTransactions)
		}
	}
}

pub struct BlockSigops<'a> {
	block: &'a IndexedBlock,
	max_sigops: usize,
}

impl<'a> BlockSigops<'a> {
	fn new(block: &'a IndexedBlock, max_sigops: usize) -> Self {
		BlockSigops {
			block: block,
			max_sigops: max_sigops,
		}
	}

	fn check(&self) -> Result<(), Error> {
		// We cannot know if bip16 is enabled at this point so we disable it.
		let sigops = self.block.transactions.iter()
			.map(|tx| transaction_sigops(&tx.raw, &NoopStore, false, false))
			.sum::<usize>();

		if sigops > self.max_sigops {
			Err(Error::MaximumSigops)
		} else {
			Ok(())
		}
	}
}

pub struct BlockMerkleRoot<'a> {
	block: &'a IndexedBlock,
}

impl<'a> BlockMerkleRoot<'a> {
	fn new(block: &'a IndexedBlock) -> Self {
		BlockMerkleRoot {
			block: block,
		}
	}

	fn check(&self) -> Result<(), Error> {
		if self.block.merkle_root() == self.block.header.raw.merkle_root_hash {
			Ok(())
		} else {
			Err(Error::MerkleRoot)
		}
	}
}

