use primitives::compact::Compact;
use chain::IndexedBlockHeader;
use network::Magic;
use work::is_valid_proof_of_work;
use constants::BLOCK_MAX_FUTURE;
use db::VerificationError;

pub struct HeaderVerifier<'a> {
	pub proof_of_work: HeaderProofOfWork<'a>,
	pub timestamp: HeaderTimestamp<'a>,
}

impl<'a> HeaderVerifier<'a> {
	pub fn new(header: &'a IndexedBlockHeader, network: Magic, current_time: u32) -> Self {
		HeaderVerifier {
			proof_of_work: HeaderProofOfWork::new(header, network),
			timestamp: HeaderTimestamp::new(header, current_time, BLOCK_MAX_FUTURE as u32),
		}
	}

	pub fn check(&self) -> Result<(), VerificationError> {
		try!(self.proof_of_work.check());
		try!(self.timestamp.check());
		Ok(())
	}
}

pub trait HeaderRule {
	fn check(&self) -> Result<(), VerificationError>;
}

pub struct HeaderProofOfWork<'a> {
	header: &'a IndexedBlockHeader,
	max_work_bits: Compact,
}

impl<'a> HeaderProofOfWork<'a> {
	fn new(header: &'a IndexedBlockHeader, network: Magic) -> Self {
		HeaderProofOfWork {
			header: header,
			max_work_bits: network.max_bits(),
		}
	}
}

impl<'a> HeaderRule for HeaderProofOfWork<'a> {
	fn check(&self) -> Result<(), VerificationError> {
		if is_valid_proof_of_work(self.max_work_bits, self.header.raw.bits, &self.header.hash) {
			Ok(())
		} else {
			Err(VerificationError::Pow)
		}
	}
}

pub struct HeaderTimestamp<'a> {
	header: &'a IndexedBlockHeader,
	current_time: u32,
	max_future: u32,
}

impl<'a> HeaderTimestamp<'a> {
	fn new(header: &'a IndexedBlockHeader, current_time: u32, max_future: u32) -> Self {
		HeaderTimestamp {
			header: header,
			current_time: current_time,
			max_future: max_future,
		}
	}
}

impl<'a> HeaderRule for HeaderTimestamp<'a> {
	fn check(&self) -> Result<(), VerificationError> {
		if self.header.raw.time > self.current_time + self.max_future {
			Err(VerificationError::FuturisticTimestamp)
		} else {
			Ok(())
		}
	}
}
