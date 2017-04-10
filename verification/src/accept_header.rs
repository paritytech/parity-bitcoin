use network::Magic;
use db::BlockHeaderProvider;
use canon::CanonHeader;
use constants::MIN_BLOCK_VERSION;
use error::Error;
use work::work_required;
use timestamp::median_timestamp;

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
			median_timestamp: HeaderMedianTimestamp::new(header, store, network),
		}
	}

	pub fn check(&self) -> Result<(), Error> {
		try!(self.version.check());
		try!(self.work.check());
		try!(self.median_timestamp.check());
		Ok(())
	}
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

	fn check(&self) -> Result<(), Error> {
		let previous_header_hash = self.header.raw.previous_header_hash.clone();
		let time = self.header.raw.time;
		let work = work_required(previous_header_hash, time, self.height, self.store, self.network);
		if work == self.header.raw.bits {
			Ok(())
		} else {
			Err(Error::Difficulty { expected: work, actual: self.header.raw.bits })
		}
	}
}

pub struct HeaderMedianTimestamp<'a> {
	header: CanonHeader<'a>,
	store: &'a BlockHeaderProvider,
	network: Magic,
}

impl<'a> HeaderMedianTimestamp<'a> {
	fn new(header: CanonHeader<'a>, store: &'a BlockHeaderProvider, network: Magic) -> Self {
		HeaderMedianTimestamp {
			header: header,
			store: store,
			network: network,
		}
	}

	fn check(&self) -> Result<(), Error> {
		let median = median_timestamp(&self.header.raw, self.store, self.network);
		if self.header.raw.time <= median {
			Err(Error::Timestamp)
		} else {
			Ok(())
		}
	}
}
