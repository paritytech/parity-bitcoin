use network::ConsensusParams;
use db::BlockHeaderProvider;
use canon::CanonHeader;
use error::Error;
use work::work_required;
use deployments::BlockDeployments;
use timestamp::median_timestamp;

pub struct HeaderAcceptor<'a> {
	pub version: HeaderVersion<'a>,
	pub work: HeaderWork<'a>,
	pub median_timestamp: HeaderMedianTimestamp<'a>,
}

impl<'a> HeaderAcceptor<'a> {
	pub fn new(
		store: &'a BlockHeaderProvider,
		consensus: &'a ConsensusParams,
		header: CanonHeader<'a>,
		height: u32,
		deployments: &'a BlockDeployments<'a>,
	) -> Self {
		HeaderAcceptor {
			work: HeaderWork::new(header, store, height, consensus),
			median_timestamp: HeaderMedianTimestamp::new(header, store, deployments),
			version: HeaderVersion::new(header, height, consensus),
		}
	}

	pub fn check(&self) -> Result<(), Error> {
		try!(self.version.check());
		try!(self.work.check());
		try!(self.median_timestamp.check());
		Ok(())
	}
}

/// Conforms to BIP90
/// https://github.com/bitcoin/bips/blob/master/bip-0090.mediawiki
pub struct HeaderVersion<'a> {
	header: CanonHeader<'a>,
	height: u32,
	consensus_params: &'a ConsensusParams,
}

impl<'a> HeaderVersion<'a> {
	fn new(header: CanonHeader<'a>, height: u32, consensus_params: &'a ConsensusParams) -> Self {
		HeaderVersion {
			header: header,
			height: height,
			consensus_params: consensus_params,
		}
	}

	fn check(&self) -> Result<(), Error> {
		if (self.header.raw.version < 2 && self.height >= self.consensus_params.bip34_height) ||
			(self.header.raw.version < 3 && self.height >= self.consensus_params.bip66_height) ||
			(self.header.raw.version < 4 && self.height >= self.consensus_params.bip65_height) {
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
	consensus: &'a ConsensusParams,
}

impl<'a> HeaderWork<'a> {
	fn new(header: CanonHeader<'a>, store: &'a BlockHeaderProvider, height: u32, consensus: &'a ConsensusParams) -> Self {
		HeaderWork {
			header: header,
			store: store,
			height: height,
			consensus: consensus,
		}
	}

	fn check(&self) -> Result<(), Error> {
		let previous_header_hash = self.header.raw.previous_header_hash.clone();
		let time = self.header.raw.time;
		let work = work_required(previous_header_hash, time, self.height, self.store, self.consensus);
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
	active: bool,
}

impl<'a> HeaderMedianTimestamp<'a> {
	fn new(header: CanonHeader<'a>, store: &'a BlockHeaderProvider, deployments: &'a BlockDeployments<'a>) -> Self {
		let active = deployments.csv();
		HeaderMedianTimestamp {
			header: header,
			store: store,
			active: active,
		}
	}

	fn check(&self) -> Result<(), Error> {
		if self.active && self.header.raw.time <= median_timestamp(&self.header.raw, self.store) {
			Err(Error::Timestamp)
		} else {
			Ok(())
		}
	}
}
