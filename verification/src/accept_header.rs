use std::cmp;
use std::collections::BTreeSet;
use network::Magic;
use db::BlockHeaderProvider;
use canon::{CanonHeader, EXPECT_CANON};
use constants::MIN_BLOCK_VERSION;
use error::Error;
use utils::work_required;

pub struct HeaderAcceptor<'a> {
	pub version: HeaderVersion<'a>,
	pub work: HeaderWork<'a>,
	pub median_timestamp: HeaderMedianTimestamp<'a>,
}

impl<'a> HeaderAcceptor<'a> {
	pub fn new(store: &'a BlockHeaderProvider, network: Magic, header: CanonHeader<'a>, height: u32) -> Self {
		HeaderAcceptor {
			// TODO: check last 1000 blocks instead of hardcoding the value
			version: HeaderVersion::new(header, MIN_BLOCK_VERSION),
			work: HeaderWork::new(header, store, height, network),
			median_timestamp: HeaderMedianTimestamp::new(header, store, height, network),
		}
	}

	pub fn check(&self) -> Result<(), Error> {
		try!(self.version.check());
		try!(self.work.check());
		try!(self.median_timestamp.check());
		Ok(())
	}
}

pub trait HeaderRule {
	fn check(&self) -> Result<(), Error>;
}

pub struct HeaderVersion<'a> {
	header: CanonHeader<'a>,
	min_version: u32,
}

impl<'a> HeaderVersion<'a> {
	fn new(header: CanonHeader<'a>, min_version: u32) -> Self {
		HeaderVersion {
			header: header,
			min_version: min_version,
		}
	}
}

impl<'a> HeaderRule for HeaderVersion<'a> {
	fn check(&self) -> Result<(), Error> {
		if self.header.raw.version < self.min_version {
			Err(Error::OldVersionBlock)
		} else {
			Ok(())
		}
	}
}

pub struct HeaderWork<'a> {
	header: CanonHeader<'a>,
	store: &'a BlockHeaderProvider,
	height: u32,
	network: Magic,
}

impl<'a> HeaderWork<'a> {
	fn new(header: CanonHeader<'a>, store: &'a BlockHeaderProvider, height: u32, network: Magic) -> Self {
		HeaderWork {
			header: header,
			store: store,
			height: height,
			network: network,
		}
	}
}

impl<'a> HeaderRule for HeaderWork<'a> {
	fn check(&self) -> Result<(), Error> {
		let previous_header_hash = self.header.raw.previous_header_hash.clone();
		let time = self.header.raw.time;
		let work = work_required(previous_header_hash, time, self.height, self.store, self.network);
		if work == self.header.raw.bits {
			Ok(())
		} else {
			Err(Error::Difficulty)
		}
	}
}

pub struct HeaderMedianTimestamp<'a> {
	header: CanonHeader<'a>,
	store: &'a BlockHeaderProvider,
	height: u32,
	network: Magic,
}

impl<'a> HeaderMedianTimestamp<'a> {
	fn new(header: CanonHeader<'a>, store: &'a BlockHeaderProvider, height: u32, network: Magic) -> Self {
		HeaderMedianTimestamp {
			header: header,
			store: store,
			height: height,
			network: network,
		}
	}
}

impl<'a> HeaderRule for HeaderMedianTimestamp<'a> {
	fn check(&self) -> Result<(), Error> {
		// TODO: timestamp validation on testnet is broken
		if self.height == 0 || self.network == Magic::Testnet {
			return Ok(());
		}

		let ancestors = cmp::min(11, self.height);
		let mut timestamps = BTreeSet::new();
		let mut block_ref = self.header.raw.previous_header_hash.clone().into();

		for _ in 0..ancestors {
			let previous_header = self.store.block_header(block_ref).expect(EXPECT_CANON);
			timestamps.insert(previous_header.time);
			block_ref = previous_header.previous_header_hash.into();
		}

		let timestamps = timestamps.into_iter().collect::<Vec<_>>();
		let median = timestamps[timestamps.len() / 2];

		if self.header.raw.time <= median {
			Err(Error::Timestamp)
		} else {
			Ok(())
		}
	}
}
