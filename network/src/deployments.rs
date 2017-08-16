const VERSIONBITS_TOP_MASK: u32 = 0xe0000000;
const VERSIONBITS_TOP_BITS: u32 = 0x20000000;

#[derive(Debug, Clone, Copy)]
pub struct Deployment {
	/// Deployment's name
	pub name: &'static str,
	/// Bit
	pub bit: u8,
	/// Start time
	pub start_time: u32,
	/// Timeout
	pub timeout: u32,
	/// Activation block number (if already activated)
	pub activation: Option<u32>,
}

/// Deployments state.
pub trait Deployments {
	/// Is deployment currently active?
	fn is_active(&self, name: &str) -> bool;
}

impl Deployment {
	pub fn matches(&self, version: u32) -> bool {
		(version & VERSIONBITS_TOP_MASK) == VERSIONBITS_TOP_BITS && (version & (1 << self.bit)) != 0
	}
}

#[cfg(test)]
pub mod tests {
	use super::Deployments;

	#[derive(Default, Debug)]
	pub struct DummyDeployments {
		pub segwit_active: bool,
	}

	impl DummyDeployments {
		pub fn deployed() -> Self {
			DummyDeployments {
				segwit_active: true,
			}
		}
	}

	impl Deployments for DummyDeployments {
		fn is_active(&self, name: &str) -> bool {
			match name {
				"segwit" => self.segwit_active,
				_ => false,
			}
		}
	}
}
