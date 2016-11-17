use primitives::hash::H256;
use std;

#[derive(Debug)]
/// Database error
pub enum Error {
	/// Rocksdb error
	DB(String),
	/// Io error
	Io(std::io::Error),
	/// Invalid meta info (while opening the database)
	Meta(MetaError),
	/// Database blockchain consistency error
	Consistency(ConsistencyError),
}

impl Error {
	pub fn unknown_hash(h: &H256) -> Self {
		Error::Consistency(ConsistencyError::Unknown(h.clone()))
	}

	pub fn unknown_number(n: u32) -> Self {
		Error::Consistency(ConsistencyError::UnknownNumber(n))
	}

	pub fn double_spend(h: &H256) -> Self {
		Error::Consistency(ConsistencyError::DoubleSpend(h.clone()))
	}

	pub fn not_main(h: &H256) -> Self {
		Error::Consistency(ConsistencyError::NotMain(h.clone()))
	}

	pub fn reorganize(h: &H256) -> Self {
		Error::Consistency(ConsistencyError::Reorganize(h.clone()))
	}
}

#[derive(Debug, PartialEq)]
pub enum ConsistencyError {
	/// Unknown hash
	Unknown(H256),
	/// Unknown number
	UnknownNumber(u32),
	/// Not the block from the main chain
	NotMain(H256),
	/// Fork too long
	ForkTooLong,
	/// Main chain block transaction attempts to double-spend
	DoubleSpend(H256),
	/// Transaction tries to spend
	UnknownSpending(H256),
	/// Chain has no best block
	NoBestBlock,
	/// Failed reorganization caused by block
	Reorganize(H256),
}


#[derive(Debug, PartialEq)]
pub enum MetaError {
	UnsupportedVersion,
}

impl From<String> for Error {
	fn from(err: String) -> Error {
		Error::DB(err)
	}
}

impl From<std::io::Error> for Error {
	fn from(err: std::io::Error) -> Error {
		Error::Io(err)
	}
}
