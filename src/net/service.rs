use stream::{Serializable, Stream};
use reader::{Deserializable, Reader, Error as ReaderError};

#[derive(Debug, Default, PartialEq, Clone)]
pub struct ServiceFlags {
	pub network: bool,
	pub getutxo: bool,
	pub bloom: bool,
	pub witness: bool,
	pub xthin: bool,
}

impl<'a> From<&'a ServiceFlags> for u64 {
	fn from(s: &'a ServiceFlags) -> Self {
		let mut result = 0u64;

		if s.network {
			result |= 1 << 0;
		}

		if s.getutxo {
			result |= 1 << 1;
		}

		if s.bloom {
			result |= 1 << 2;
		}

		if s.witness {
			result |= 1 << 3;
		}

		if s.xthin {
			result |= 1 << 4;
		}

		result
	}
}

// TODO: do we need to throw err on unknown service flags?
impl From<u64> for ServiceFlags {
	fn from(v: u64) -> Self {
		ServiceFlags {
			network: v & 0x1 != 0,
			getutxo: v & 0x10 != 0,
			bloom: v & 0x100 != 0,
			witness: v & 0x1000 != 0,
			xthin: v & 0x10000 != 0,
		}
	}
}

impl ServiceFlags {
	pub fn network(mut self, v: bool) -> Self {
		self.network = v;
		self
	}

	pub fn getutxo(mut self, v: bool) -> Self {
		self.getutxo = v;
		self
	}

	pub fn bloom(mut self, v: bool) -> Self {
		self.bloom = v;
		self
	}

	pub fn witness(mut self, v: bool) -> Self {
		self.witness = v;
		self
	}

	pub fn xthin(mut self, v: bool) -> Self {
		self.xthin = v;
		self
	}
}

impl Serializable for ServiceFlags {
	fn serialize(&self, stream: &mut Stream) {
		stream.append(&u64::from(self));
	}
}

impl Deserializable for ServiceFlags {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		reader.read().map(|v: u64| v.into())
	}
}
